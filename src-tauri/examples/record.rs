//! Record one connection into a session log, and optionally forward it, so the
//! real verifier can reach this code without the game.
//!
//! `apex-guideline`'s `services/liveapi/replay.py` is a fake Apex: it dials a
//! WebSocket, plays a list of events, then reads back the session log the
//! other end wrote. It was built to verify the server's collector, and
//! pointed here it verifies this app instead - which is the whole reason the
//! session format is not ours to invent.
//!
//!     cargo run --manifest-path src-tauri/Cargo.toml --example record \
//!         -- 7777 /tmp/companion-sessions
//!     python3 ../apex-guideline/services/liveapi/replay.py \
//!         --port 7777 --root /tmp/companion-sessions
//!
//! Given a third argument it also forwards every frame to a collector, and the
//! same verifier then checks the far end of the relay - the collector writes a
//! session log of its own, in the same format, from the frames this app sent
//! it:
//!
//!     python3 ../apex-guideline/services/liveapi/collector.py \
//!         --port 7788 --root /tmp/upstream-check &
//!     cargo run --manifest-path src-tauri/Cargo.toml --example record \
//!         -- 7777 /tmp/companion-sessions ws://127.0.0.1:7788/
//!     python3 ../apex-guideline/services/liveapi/replay.py \
//!         --port 7777 --root /tmp/upstream-check
//!
//! This is not the app's wiring. The app owns consent, where sessions live and
//! when the upstream is allowed to exist, and none of that belongs in a
//! harness.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cryptocompass_companion_lib::listen::{bind, serve, Frame};
use cryptocompass_companion_lib::session::SessionLog;
use cryptocompass_companion_lib::upstream::Upstream;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let usage = "usage: record <port> <session root> [collector url]";
    let port: u16 = args.next().expect(usage).parse().expect("port is a number");
    let root = PathBuf::from(args.next().expect(usage));
    let collector = args.next();

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = bind(addr).await.expect("could not take the port");
    println!("listening on ws://{addr}/, writing into {}", root.display());

    if let Some(url) = &collector {
        println!("forwarding to {url}");
    }

    let mut log: Option<SessionLog> = None;
    let mut upstream: Option<Upstream> = None;

    serve(listener, move |frame| {
        match frame {
            Frame::Text(text) => {
                // The disk first and the network second, in that order and not
                // the other way round: a collector that is down must cost the
                // match nothing.
                open(&mut log, &root)
                    .write_text(&text)
                    .expect("write failed");

                if let Some(url) = &collector {
                    upstream
                        .get_or_insert_with(|| Upstream::start(url.clone()))
                        .send(text);
                }
            }
            // Not forwarded: a binary frame is one this stack cannot read, and
            // the local row says as much without decoding it.
            Frame::Binary(bytes) => {
                open(&mut log, &root)
                    .write_binary(bytes.len())
                    .expect("write failed");
            }
            Frame::Closed => {
                match log.take().map(SessionLog::close) {
                    Some(Ok(path)) => println!("wrote {}", path.display()),
                    Some(Err(error)) => println!("nothing recorded: {error}"),
                    None => println!("connection closed before a frame arrived"),
                }

                if let Some(sender) = upstream.take() {
                    // The game's connection ended, so this one does too - the
                    // collector writes one file per connection. Dropping the
                    // handle hangs up, so the queue is given a moment to empty
                    // first.
                    tokio::spawn(async move {
                        for _ in 0..100 {
                            if sender.pending() == 0 {
                                break;
                            }

                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }

                        println!("upstream finished: {:?}", sender.state());
                    });
                }
            }
        };
    })
    .await
    .expect("the listener stopped");
}

/// The session is minted on the connection's first frame, the way the app
/// will mint it: the local file and the uploaded stream share the name.
fn open<'a>(log: &'a mut Option<SessionLog>, root: &Path) -> &'a mut SessionLog {
    log.get_or_insert_with(|| {
        let session_id = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3f").to_string();
        SessionLog::create(root, &session_id).expect("could not open the session log")
    })
}
