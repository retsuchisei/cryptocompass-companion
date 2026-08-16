//! Record one connection into a session log, so the real verifier can reach
//! this code without the game.
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
//! This is not the app's wiring. The app owns consent, where sessions live
//! and an upstream, and none of that belongs in a harness.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use cryptocompass_companion_lib::listen::{bind, serve, Frame};
use cryptocompass_companion_lib::session::SessionLog;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let usage = "usage: record <port> <session root>";
    let port: u16 = args.next().expect(usage).parse().expect("port is a number");
    let root = PathBuf::from(args.next().expect(usage));

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = bind(addr).await.expect("could not take the port");
    println!("listening on ws://{addr}/, writing into {}", root.display());

    let mut log: Option<SessionLog> = None;

    serve(listener, move |frame| {
        match frame {
            Frame::Text(text) => {
                open(&mut log, &root)
                    .write_text(&text)
                    .expect("write failed");
            }
            Frame::Binary(bytes) => {
                open(&mut log, &root)
                    .write_binary(bytes.len())
                    .expect("write failed");
            }
            Frame::Closed => match log.take().map(SessionLog::close) {
                Some(Ok(path)) => println!("wrote {}", path.display()),
                Some(Err(error)) => println!("nothing recorded: {error}"),
                None => println!("connection closed before a frame arrived"),
            },
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
