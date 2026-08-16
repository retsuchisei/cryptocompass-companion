//! The other half of the relay: what the game said, said onward to the
//! collector.
//!
//! Frames go up verbatim, exactly as the game would have sent them, so the
//! collector needs no change to accept them and writes its own session log per
//! connection - a dropped link means a reconnect and a second file on the
//! server, which is a case `session.py` already expects.
//!
//! There is no acknowledgement in v1. A frame handed to a socket that then
//! dies may or may not have landed, and this end cannot tell. It is therefore
//! kept in the queue until its send returns, so a drop costs a duplicate
//! rather than a gap: the observation layer dedups by `obsId`, and nothing
//! anywhere recovers a gap.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// The longest wait between attempts. Long enough to be polite to a collector
/// that is down, short enough that a match recorded during an outage is not
/// stuck behind the wait once it comes back.
const BACKOFF_CAP_SECONDS: u64 = 60;

/// What the interface has to say about the link, and nothing more: counts and
/// waits, never what the frames meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamState {
    /// Nothing to carry yet, so no connection is held. The collector is dialled
    /// when there is something to send it, not when recording is switched on:
    /// an app left running with the game closed otherwise reconnects at the
    /// collector forever with nothing to say.
    Idle,
    Connecting,
    Live {
        sent: u64,
    },
    Retrying {
        after_seconds: u64,
        failures: u32,
    },
}

/// A handle on the task that owns the connection.
pub struct Upstream {
    shared: Arc<Shared>,
}

struct Shared {
    /// Frames accepted and not yet gone. Unbounded on purpose: the listener
    /// must never wait on the network, because what it is reading cannot be
    /// replayed, and a bounded queue would make it choose what to drop.
    queue: Mutex<VecDeque<String>>,
    state: Mutex<UpstreamState>,
    /// Wakes the sender: a frame arrived, or the handle went away.
    wake: Notify,
    stopped: AtomicBool,
}

impl Upstream {
    /// Start the sender. It runs until the handle is dropped, so the caller
    /// holds one for exactly as long as recording is on.
    ///
    /// Call it from inside a tokio runtime; Tauri's is one.
    pub fn start(url: String) -> Upstream {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            state: Mutex::new(UpstreamState::Idle),
            wake: Notify::new(),
            stopped: AtomicBool::new(false),
        });

        tokio::spawn(run(url, shared.clone()));
        Upstream { shared }
    }

    /// Hand over a frame. Never blocks and cannot fail - what the network then
    /// does with it is the sender's problem, not the listener's.
    pub fn send(&self, raw: String) {
        self.shared.queue.lock().unwrap().push_back(raw);
        self.shared.wake.notify_one();
    }

    pub fn state(&self) -> UpstreamState {
        *self.shared.state.lock().unwrap()
    }

    /// Frames taken and not yet sent.
    pub fn pending(&self) -> usize {
        self.shared.queue.lock().unwrap().len()
    }
}

impl Drop for Upstream {
    fn drop(&mut self) {
        // Recording going off has to stop the upload with it. A task left
        // running would go on emptying the queue at the collector after the
        // person at the keyboard said stop, and would be joined by a second
        // one the next time recording went on.
        self.shared.stopped.store(true, Ordering::Relaxed);
        self.shared.wake.notify_one();
    }
}

impl Shared {
    /// Park until there is a frame to carry, reporting Idle while parked.
    async fn wait_for_work(&self) {
        loop {
            if !self.queue.lock().unwrap().is_empty() || self.is_stopped() {
                return;
            }

            self.set_state(UpstreamState::Idle);
            // No wakeup is lost here: a `notify_one` with nobody waiting
            // leaves a permit, so a frame that arrives between the check and
            // this await returns from it immediately.
            self.wake.notified().await;
        }
    }

    fn set_state(&self, state: UpstreamState) {
        *self.state.lock().unwrap() = state;
    }

    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }
}

/// The task that owns the connection: dial, drain, and on losing either, wait
/// and dial again.
async fn run(url: String, shared: Arc<Shared>) {
    let mut failures: u32 = 0;
    let mut sent: u64 = 0;

    loop {
        shared.wait_for_work().await;

        if shared.is_stopped() {
            return;
        }

        shared.set_state(UpstreamState::Connecting);

        if let Ok((mut ws, _)) = tokio_tungstenite::connect_async(url.as_str()).await {
            failures = 0;
            shared.set_state(UpstreamState::Live { sent });
            pump(&mut ws, &shared, &mut sent).await;

            if shared.is_stopped() {
                return;
            }
        }

        // A refused connection and one that died mid-stream are the same thing
        // from here: the link is gone and the queue is not.
        let after_seconds = backoff_seconds(failures);
        failures += 1;
        shared.set_state(UpstreamState::Retrying {
            after_seconds,
            failures,
        });
        tokio::time::sleep(Duration::from_secs(after_seconds)).await;
    }
}

/// Drain the queue into a live connection, and return when it is no longer
/// live.
async fn pump(
    ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    shared: &Shared,
    sent: &mut u64,
) {
    loop {
        if shared.is_stopped() {
            return;
        }

        let next = shared.queue.lock().unwrap().front().cloned();

        match next {
            Some(raw) => {
                if ws.send(Message::Text(raw.into())).await.is_err() {
                    return;
                }

                // Dropped from the queue only once the send returned: a frame
                // whose send failed is still at the front and goes again on
                // the next connection.
                shared.queue.lock().unwrap().pop_front();
                *sent += 1;
                shared.set_state(UpstreamState::Live { sent: *sent });
            }
            // Nothing to send. Watching the socket even now is what keeps a
            // frame out of a dead connection: a write to a peer that has gone
            // succeeds into the kernel's buffer and is lost, so the going has
            // to be noticed here instead.
            None => tokio::select! {
                _ = shared.wake.notified() => {}
                incoming = ws.next() => match incoming {
                    // The collector says nothing this end reads. It is watched
                    // for closing, not for talking.
                    Some(Ok(_)) => {}
                    _ => return,
                },
            },
        }
    }
}

/// `min(60, 2^failures)`, where the argument is how many attempts failed
/// before this wait.
pub fn backoff_seconds(failures: u32) -> u64 {
    1u64.checked_shl(failures)
        .unwrap_or(BACKOFF_CAP_SECONDS)
        .min(BACKOFF_CAP_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// A socket nobody is listening on yet, and the url that reaches it. The
    /// port is the kernel's choice, so tests do not collide.
    async fn take_a_port() -> (TcpListener, String) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", listener.local_addr().unwrap());
        (listener, url)
    }

    /// A collector that is real enough to prove the bytes arrived: it accepts
    /// one connection and keeps every text frame until the peer goes away.
    /// A relay's correctness is what came out the other end, so the other end
    /// cannot be a mock.
    async fn accept_and_collect(listener: &TcpListener, seen: &Arc<Mutex<Vec<String>>>) {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

        while let Some(Ok(message)) = ws.next().await {
            if let Message::Text(text) = message {
                seen.lock().unwrap().push(text.to_string());
            }
        }
    }

    /// Poll until it holds. The sender runs on its own task, so a fixed sleep
    /// is either flaky or slow, and this is both shorter and steadier.
    async fn within(seconds: u64, mut settled: impl FnMut() -> bool) -> bool {
        for _ in 0..(seconds * 50) {
            if settled() {
                return true;
            }

            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        settled()
    }

    #[tokio::test]
    async fn frames_queue_while_disconnected_and_flush_on_connect() {
        let upstream = Upstream::start("ws://127.0.0.1:59999/nothing-here".into());
        upstream.send("{\"a\":1}".into());
        upstream.send("{\"a\":2}".into());

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(matches!(upstream.state(), UpstreamState::Retrying { .. }));
        assert_eq!(
            upstream.pending(),
            2,
            "nothing may be dropped while offline"
        );
    }

    #[tokio::test]
    async fn nothing_is_dialled_until_there_is_something_to_carry() {
        let (listener, url) = take_a_port().await;
        let upstream = Upstream::start(url);

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(upstream.state(), UpstreamState::Idle);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "the collector was dialled with nothing to send it"
        );
    }

    #[tokio::test]
    async fn frames_arrive_in_the_order_they_were_given_and_are_counted() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (listener, url) = take_a_port().await;
        let collector = seen.clone();
        tokio::spawn(async move { accept_and_collect(&listener, &collector).await });

        let upstream = Upstream::start(url);
        upstream.send("one".into());
        upstream.send("two".into());
        upstream.send("three".into());

        assert!(
            within(5, || upstream.state() == UpstreamState::Live { sent: 3 }).await,
            "three frames went in, {:?} came out",
            upstream.state()
        );
        assert_eq!(*seen.lock().unwrap(), vec!["one", "two", "three"]);
        assert_eq!(upstream.pending(), 0, "a sent frame is not still waiting");
    }

    #[tokio::test]
    async fn a_peer_that_went_away_is_noticed_before_a_frame_is_written_into_it() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (listener, url) = take_a_port().await;
        let collector = seen.clone();

        tokio::spawn(async move {
            // A collector that takes one frame and hangs up without a close
            // frame, which is what a restarted one looks like from here.
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

            if let Some(Ok(Message::Text(text))) = ws.next().await {
                collector.lock().unwrap().push(text.to_string());
            }

            drop(ws);
            accept_and_collect(&listener, &collector).await;
        });

        let upstream = Upstream::start(url);
        upstream.send("one".into());
        assert!(
            within(5, || seen.lock().unwrap().len() == 1).await,
            "the first frame never arrived"
        );

        // Long enough for the sender to see the socket go.
        tokio::time::sleep(Duration::from_millis(500)).await;
        upstream.send("two".into());

        assert!(
            within(10, || seen.lock().unwrap().len() == 2).await,
            "a frame handed over after the peer went away was written into the \
             dead socket and lost"
        );
        assert_eq!(*seen.lock().unwrap(), vec!["one", "two"]);
    }

    #[tokio::test]
    async fn dropping_the_handle_hangs_up() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (listener, url) = take_a_port().await;
        let collector = seen.clone();
        let hung_up = Arc::new(AtomicBool::new(false));
        let noticed = hung_up.clone();

        tokio::spawn(async move {
            accept_and_collect(&listener, &collector).await;
            noticed.store(true, Ordering::Relaxed);
        });

        let upstream = Upstream::start(url);
        upstream.send("one".into());
        assert!(within(5, || seen.lock().unwrap().len() == 1).await);

        // Turning recording off drops the handle, and nothing may go upstream
        // after that - including what is already queued.
        drop(upstream);

        assert!(
            within(5, || hung_up.load(Ordering::Relaxed)).await,
            "the sender outlived its handle and is still holding the connection"
        );
    }

    #[tokio::test]
    async fn a_wss_url_reaches_the_network_rather_than_a_missing_feature() {
        // The collector is behind TLS, and a build without it fails in the one
        // way nothing above can see: `connect_async` refuses the url outright,
        // which from the sender's seat looks exactly like a collector that is
        // down - Retrying, forever, saying nothing. This is the only place the
        // difference shows.
        let (listener, url) = take_a_port().await;
        let secure = url.replacen("ws://", "wss://", 1);
        tokio::spawn(async move { drop(listener.accept().await) });

        let error = tokio_tungstenite::connect_async(secure).await.unwrap_err();
        assert!(
            !matches!(error, tokio_tungstenite::tungstenite::Error::Url(_)),
            "wss never got as far as the network: {error}"
        );
    }

    #[test]
    fn backoff_grows_and_is_capped() {
        assert_eq!(backoff_seconds(0), 1);
        assert_eq!(backoff_seconds(1), 2);
        assert_eq!(backoff_seconds(2), 4);
        assert_eq!(
            backoff_seconds(9),
            60,
            "capped, so a long outage is not a long wait"
        );
    }
}
