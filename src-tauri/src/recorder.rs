//! What the switch switches: the socket the game dials, the log it is written
//! to, and the sender that carries it onward.
//!
//! Nothing here starts on its own. A `Recorder` is off until `start` is
//! called, which is what makes "recording is off on first run and after every
//! update" true of the program rather than only of the screen.
//!
//! It holds the pieces together and nothing else - the wire is `listen`, the
//! file is `session`, the network is `upstream` - so the order they run in is
//! the one rule this module owns: the disk before the network, always.

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use serde::Serialize;
use tokio::task::JoinHandle;

use crate::listen::{bind, serve, Frame};
use crate::session::SessionLog;
use crate::upstream::{Upstream, UpstreamState};

/// How long the sender is given to empty its queue when the game hangs up,
/// polled this often. What is still waiting after that is on disk anyway, and
/// a connection held open for a collector that is down helps nobody.
const DRAIN_LIMIT: Duration = Duration::from_secs(5);
const DRAIN_POLL: Duration = Duration::from_millis(50);

/// What the interface is told. Counts and times, never what a frame meant.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub recording: bool,
    /// Unix ms when recording was turned on.
    pub since: Option<i64>,
    pub session_id: Option<String>,
    /// Frames taken from the game since recording went on. Not rows written:
    /// an Init becomes the log's header and takes no row, and a frame that
    /// went missing from the screen for that reason would be a bug report.
    pub frames: u64,
    pub last_frame_at: Option<i64>,
    pub upstream: UpstreamReport,
}

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamReport {
    /// `off` when no collector is configured at all - the app then records
    /// locally and sends nothing.
    pub state: &'static str,
    /// Frames handed to the socket. There is no acknowledgement in v1, so this
    /// is not the same as frames the collector kept.
    pub sent: u64,
    pub pending: usize,
    pub retry_in_seconds: u64,
    pub failures: u32,
}

impl UpstreamReport {
    fn quiet(state: &'static str) -> Self {
        Self {
            state,
            sent: 0,
            pending: 0,
            retry_in_seconds: 0,
            failures: 0,
        }
    }

    fn of(upstream: &Upstream) -> Self {
        let pending = upstream.pending();

        match upstream.state() {
            UpstreamState::Idle => Self {
                pending,
                ..Self::quiet("idle")
            },
            UpstreamState::Connecting => Self {
                pending,
                ..Self::quiet("connecting")
            },
            UpstreamState::Live { sent } => Self {
                sent,
                pending,
                ..Self::quiet("live")
            },
            UpstreamState::Retrying {
                after_seconds,
                failures,
            } => Self {
                pending,
                retry_in_seconds: after_seconds,
                failures,
                ..Self::quiet("retrying")
            },
        }
    }
}

/// The switch itself. One per app, held by the window.
pub struct Recorder {
    root: PathBuf,
    /// `None` is off, and off is where it starts.
    running: Mutex<Option<Running>>,
}

struct Running {
    port: u16,
    since: i64,
    task: JoinHandle<()>,
    session: Arc<Session>,
}

/// Everything one recording owns, shared with the task that fills it.
struct Session {
    root: PathBuf,
    collector: Option<String>,
    /// `None` until the game's first frame: a probe that connects and leaves
    /// should not litter the directory.
    log: Mutex<Option<SessionLog>>,
    upstream: Mutex<Option<Upstream>>,
    counters: Mutex<Counters>,
}

#[derive(Default)]
struct Counters {
    session_id: Option<String>,
    frames: u64,
    last_frame_at: Option<i64>,
}

impl Recorder {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            running: Mutex::new(None),
        }
    }

    /// Take the socket the game dials and start writing what arrives.
    /// Returns the port taken, which is the kernel's choice when `addr` names
    /// port 0. Call it from inside a tokio runtime; Tauri's is one.
    ///
    /// Starting an already started recorder changes nothing and reports the
    /// port it already holds.
    pub async fn start(&self, addr: SocketAddr, collector: Option<String>) -> io::Result<u16> {
        if let Some(running) = self.running.lock().unwrap().as_ref() {
            return Ok(running.port);
        }

        let listener = bind(addr).await?;
        let port = listener.local_addr()?.port();
        let session = Arc::new(Session {
            root: self.root.clone(),
            collector,
            log: Mutex::new(None),
            upstream: Mutex::new(None),
            counters: Mutex::new(Counters::default()),
        });

        let sink = session.clone();
        // Checked again now the lock is held and cannot be awaited across: two
        // presses landing on two runtime threads must not leave two listeners.
        let mut running = self.running.lock().unwrap();

        if let Some(existing) = running.as_ref() {
            return Ok(existing.port);
        }

        let task = tokio::spawn(async move {
            // `serve` returns only if the socket itself fails. Stopping is the
            // other way out, and it aborts this task.
            if let Err(error) = serve(listener, move |frame| sink.on_frame(frame)).await {
                eprintln!("the listener stopped: {error}");
            }
        });

        *running = Some(Running {
            port,
            since: now_ms(),
            task,
            session,
        });

        Ok(port)
    }

    /// Off, and off means off: the socket is given back, the log is finished
    /// and the sender hangs up with whatever it was still holding. The person
    /// at the keyboard said stop, so nothing goes upstream after this.
    pub fn stop(&self) {
        let Some(running) = self.running.lock().unwrap().take() else {
            return;
        };

        // The listening socket is owned by the task, so aborting it is what
        // frees the port.
        running.task.abort();
        running.session.finish();
    }

    pub fn status(&self) -> Status {
        let held = self.running.lock().unwrap();

        let Some(running) = held.as_ref() else {
            return Status {
                recording: false,
                since: None,
                session_id: None,
                frames: 0,
                last_frame_at: None,
                upstream: UpstreamReport::quiet("off"),
            };
        };

        let counters = running.session.counters.lock().unwrap();

        Status {
            recording: true,
            since: Some(running.since),
            session_id: counters.session_id.clone(),
            frames: counters.frames,
            last_frame_at: counters.last_frame_at,
            upstream: running.session.upstream_report(),
        }
    }
}

impl Session {
    fn on_frame(&self, frame: Frame) {
        match frame {
            Frame::Text(text) => {
                // The disk first and the network second, in that order and not
                // the other way round: a collector that is down must cost the
                // match nothing.
                self.write(|log| log.write_text(&text));
                self.count();
                self.forward(text);
            }
            // Not forwarded: a binary frame is one this stack cannot read, and
            // the local row says as much without decoding it.
            Frame::Binary(bytes) => {
                self.write(|log| log.write_binary(bytes.len()));
                self.count();
            }
            Frame::Closed => self.end_of_connection(),
        }
    }

    /// Write one row, opening the log - and minting the session id - on the
    /// first frame of a connection.
    fn write(&self, row: impl FnOnce(&mut SessionLog) -> io::Result<()>) {
        let mut held = self.log.lock().unwrap();

        if held.is_none() {
            let session_id = new_session_id();

            match SessionLog::create(&self.root, &session_id) {
                Ok(log) => {
                    self.counters.lock().unwrap().session_id = Some(session_id);
                    *held = Some(log);
                }
                Err(error) => {
                    // Recording carries on: the upload is still worth having,
                    // and the alternative is a panic that takes the listener
                    // with it.
                    eprintln!("no session log in {}: {error}", self.root.display());
                    return;
                }
            }
        }

        if let Err(error) = row(held.as_mut().expect("the log was just opened")) {
            eprintln!("session log: {error}");
        }
    }

    fn count(&self) {
        let mut counters = self.counters.lock().unwrap();
        counters.frames += 1;
        counters.last_frame_at = Some(now_ms());
    }

    fn forward(&self, text: String) {
        let Some(url) = &self.collector else {
            return;
        };

        self.upstream
            .lock()
            .unwrap()
            .get_or_insert_with(|| Upstream::start(url.clone()))
            .send(text);
    }

    /// The game hung up. The log is finished here rather than at stop, because
    /// a session log covers one connection - and so does the collector's, so
    /// the upload ends with it too.
    fn end_of_connection(&self) {
        self.close_log();

        if let Some(sender) = self.upstream.lock().unwrap().take() {
            // Dropping the handle hangs up, so the end of the match is given
            // its moment to leave first.
            tokio::spawn(async move {
                for _ in 0..(DRAIN_LIMIT.as_millis() / DRAIN_POLL.as_millis()) {
                    if sender.pending() == 0 {
                        break;
                    }

                    tokio::time::sleep(DRAIN_POLL).await;
                }
            });
        }
    }

    fn finish(&self) {
        self.close_log();
        drop(self.upstream.lock().unwrap().take());
    }

    fn close_log(&self) {
        let Some(log) = self.log.lock().unwrap().take() else {
            return;
        };

        match log.close() {
            Ok(path) => println!("wrote {}", path.display()),
            // Including the ordinary case of a connection that carried
            // nothing, which has no file to name.
            Err(error) => eprintln!("session not written: {error}"),
        }
    }

    fn upstream_report(&self) -> UpstreamReport {
        if self.collector.is_none() {
            return UpstreamReport::quiet("off");
        }

        match self.upstream.lock().unwrap().as_ref() {
            // Configured, but the collector is not dialled until there is
            // something to carry.
            None => UpstreamReport::quiet("idle"),
            Some(upstream) => UpstreamReport::of(upstream),
        }
    }
}

/// The app mints it, not the server, so the local file and the uploaded stream
/// can be matched up by hand.
fn new_session_id() -> String {
    Utc::now().format("%Y%m%dT%H%M%S%.3f").to_string()
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

    /// Any free port on loopback, chosen by the kernel so tests do not collide.
    fn anywhere_on_loopback() -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], 0))
    }

    /// Apex, as far as this end can tell: something that dials the port and
    /// sends text frames.
    async fn game_connects(port: u16) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/"))
            .await
            .unwrap()
            .0
    }

    /// Poll until it holds. The recorder runs on its own task, so a fixed sleep
    /// is either flaky or slow.
    async fn within(seconds: u64, mut settled: impl FnMut() -> bool) -> bool {
        for _ in 0..(seconds * 50) {
            if settled() {
                return true;
            }

            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        settled()
    }

    async fn eventually_refused(port: u16) -> bool {
        for _ in 0..250 {
            if TcpStream::connect(("127.0.0.1", port)).await.is_err() {
                return true;
            }

            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        false
    }

    fn files_in(dir: &std::path::Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect()
    }

    #[tokio::test]
    async fn a_recorder_that_was_never_started_is_off() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new(dir.path().to_path_buf());

        assert!(!recorder.status().recording);
        assert_eq!(recorder.status().upstream.state, "off");

        // A window closing stops a recorder that may never have started, and
        // that must cost nothing.
        recorder.stop();

        assert!(!recorder.status().recording);
        assert!(
            files_in(dir.path()).is_empty(),
            "a recorder that was never on wrote something"
        );
    }

    #[tokio::test]
    async fn turning_it_on_takes_the_socket_and_turning_it_off_gives_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new(dir.path().to_path_buf());
        let port = recorder.start(anywhere_on_loopback(), None).await.unwrap();

        let mut game = game_connects(port).await;
        game.send(Message::Text(r#"{"category":"init","name":"t"}"#.into()))
            .await
            .unwrap();
        game.send(Message::Text(r#"{"category":"matchSetup"}"#.into()))
            .await
            .unwrap();

        // Frames the game sent, not rows written: the Init became the header
        // and would otherwise look on screen like a frame that went missing.
        assert!(
            within(5, || recorder.status().frames == 2).await,
            "two frames went in, {} were counted",
            recorder.status().frames
        );

        let status = recorder.status();
        assert!(status.recording);
        assert!(status.since.is_some(), "recording says when it started");
        assert!(status.session_id.is_some(), "the app mints the session id");
        assert!(status.last_frame_at.is_some());

        recorder.stop();
        assert!(!recorder.status().recording);

        // The log is finished rather than left open: `.jsonl.gz` is the name
        // replay.py looks for, and a plain `.jsonl` is an unclosed session.
        let written = files_in(dir.path());
        assert_eq!(written.len(), 1, "{written:?}");
        assert!(written[0].ends_with(".jsonl.gz"), "{written:?}");

        assert!(
            eventually_refused(port).await,
            "the socket outlived the switch, so off does not mean off"
        );
    }

    #[tokio::test]
    async fn starting_twice_holds_one_socket() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new(dir.path().to_path_buf());

        let first = recorder.start(anywhere_on_loopback(), None).await.unwrap();
        let second = recorder.start(anywhere_on_loopback(), None).await.unwrap();

        // A second press of a button is not a second listener, and not an
        // orphaned task holding the first session either.
        assert_eq!(first, second);

        let mut game = game_connects(first).await;
        game.send(Message::Text(r#"{"category":"matchSetup"}"#.into()))
            .await
            .unwrap();
        assert!(within(5, || recorder.status().frames == 1).await);

        recorder.stop();
        assert!(eventually_refused(first).await);
    }

    #[tokio::test]
    async fn the_tail_of_a_match_reaches_the_collector() {
        let collector = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", collector.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let received = seen.clone();

        tokio::spawn(async move {
            let (stream, _) = collector.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

            while let Some(Ok(message)) = ws.next().await {
                if let Message::Text(text) = message {
                    received.lock().unwrap().push(text.to_string());
                }
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new(dir.path().to_path_buf());
        let port = recorder
            .start(anywhere_on_loopback(), Some(url))
            .await
            .unwrap();

        let mut game = game_connects(port).await;
        for frame in [r#"{"category":"init"}"#, r#"{"a":1}"#, r#"{"a":2}"#] {
            game.send(Message::Text(frame.into())).await.unwrap();
        }

        // The game hanging up ends the upload with it, because the collector
        // writes one file per connection. What is still queued at that moment
        // is the end of a match, and it goes before the link does.
        game.close(None).await.unwrap();

        assert!(
            within(10, || seen.lock().unwrap().len() == 3).await,
            "the collector saw {:?}",
            seen.lock().unwrap()
        );
        assert_eq!(
            *seen.lock().unwrap(),
            vec![r#"{"category":"init"}"#, r#"{"a":1}"#, r#"{"a":2}"#],
            "frames go up verbatim, in order"
        );

        recorder.stop();
    }
}
