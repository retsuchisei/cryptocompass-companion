//! Explicit CLI recording for native capture/replay acceptance, using the app's
//! real recorder, journal and switch ownership. No game semantics are decoded.
//!
//! cargo run --manifest-path src-tauri/Cargo.toml --example record -- 7777 /tmp/sessions [collector]
//!
//! CRYPTOCOMPASS_TOKEN supplies the existing install bearer through environment,
//! never the command line or a URL. CRYPTOCOMPASS_RECORD_SECONDS optionally
//! bounds an acceptance run; otherwise Ctrl-C is the recording-off action.
//! The separate replay verifier can still read the unchanged legacy exports.

use cryptocompass_companion_lib::recorder::Recorder;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let usage = "usage: record <port> <session root> [collector url]";
    let port: u16 = args.next().expect(usage).parse().expect("Invalid port");
    let root = PathBuf::from(args.next().expect(usage));
    let collector = args.next();
    let token = std::env::var("CRYPTOCOMPASS_TOKEN").ok();
    let seconds = std::env::var("CRYPTOCOMPASS_RECORD_SECONDS")
        .ok()
        .map(|value| value.parse::<u64>().expect("Invalid capture duration"));
    if let Some(seconds) = seconds {
        assert!((1..=3600).contains(&seconds), "Invalid capture duration");
    }
    let recorder = Recorder::new(root);
    let port = recorder
        .start(SocketAddr::from(([127, 0, 0, 1], port)), collector, token)
        .await
        .expect("Recorder start failed");
    println!("Listening on loopback port {port}; recording explicitly enabled");
    if let Some(seconds) = seconds {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
    } else {
        tokio::signal::ctrl_c().await.expect("Stop signal failed");
    }
    recorder.stop();
    println!(
        "{}",
        serde_json::to_string(&recorder.status()).expect("Invalid status")
    );
}
