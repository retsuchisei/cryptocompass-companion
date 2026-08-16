pub mod config;
pub mod identity;
pub mod listen;
pub mod recorder;
pub mod session;
pub mod upstream;

use std::net::SocketAddr;

use serde::Serialize;
use tauri::{Manager, State};

use recorder::Recorder;

/// The port the game dials. `scripts/liveapi.ps1` in `apex-guideline` already
/// uses it, and the config this app writes will name the same one.
const LOCAL_PORT: u16 = 7777;

/// Where frames go after the disk, compiled in at release time.
///
/// Absent in a build made from a clone, and absent on purpose: the ingest
/// address carries a secret path and this repository is public. Without it the
/// app records locally and sends nothing, which is a usable app rather than a
/// broken one.
const COLLECTOR_URL: Option<&str> = option_env!("CRYPTOCOMPASS_COLLECTOR_URL");

/// The socket the recorder takes, and the address the game is told to dial.
/// One constant behind both, because a config naming a port nothing holds
/// fails as silence: the game connects to nobody and reports nothing.
fn recording_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], LOCAL_PORT))
}

fn local_url() -> String {
    format!("ws://{}", recording_addr())
}

/// The recorder's own report, plus the one thing only the binary knows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppStatus {
    /// The running version. Consent is remembered against it, so an update
    /// asks again rather than inheriting an answer given to another build.
    version: &'static str,
    #[serde(flatten)]
    recorder: recorder::Status,
}

/// The only path that opens the socket. Nothing else calls `Recorder::start`,
/// and the interface reaches this through the consent screen.
#[tauri::command]
async fn start_recording(
    app: tauri::AppHandle,
    recorder: State<'_, Recorder>,
) -> Result<u16, String> {
    // Read at start rather than held in memory: an install linked while the app
    // was open should not need a restart to start sending.
    let token = identity_dir(&app)
        .and_then(|dir| identity::load(&dir))
        .map(|stored| stored.token);

    recorder
        .start(recording_addr(), COLLECTOR_URL.map(str::to_string), token)
        .await
        .map_err(failure)
}

/// Where the token lives. `None` only if the platform will not name a config
/// directory, which is a machine that cannot store anything anyway.
fn identity_dir(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok()
}

/// What the interface is allowed to know about this install: its name, never
/// its token.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Linked {
    linked: bool,
    name: Option<String>,
}

#[tauri::command]
fn linked_as(app: tauri::AppHandle) -> Linked {
    match identity_dir(&app).and_then(|dir| identity::load(&dir)) {
        Some(stored) => Linked {
            linked: true,
            name: Some(stored.name),
        },
        None => Linked {
            linked: false,
            name: None,
        },
    }
}

/// Start a pairing. The code comes back to be shown; the poll key stays in the
/// backend, where the interface cannot reach it and cannot leak it.
#[tauri::command]
async fn begin_pairing(
    app: tauri::AppHandle,
    pending: State<'_, PendingPairing>,
    name: String,
) -> Result<identity::Pairing, String> {
    let _ = &app;
    let pairing = identity::begin(&identity::site(), &name).await?;

    *pending.0.lock().unwrap() = Some((pairing.poll_key.clone(), name));

    Ok(pairing)
}

/// Has anybody confirmed yet? `false` is the ordinary answer.
#[tauri::command]
async fn poll_pairing(
    app: tauri::AppHandle,
    pending: State<'_, PendingPairing>,
) -> Result<bool, String> {
    let Some((poll_key, name)) = pending.0.lock().unwrap().clone() else {
        return Err("nothing is pairing".to_string());
    };

    let Some(token) = identity::collect(&identity::site(), &poll_key).await? else {
        return Ok(false);
    };

    let dir = identity_dir(&app).ok_or_else(|| "no config directory".to_string())?;
    identity::save(&dir, &identity::Stored { token, name }).map_err(failure)?;
    *pending.0.lock().unwrap() = None;

    Ok(true)
}

/// Forget the token on this machine. The install stays listed on the account
/// page until it is switched off there - which is the honest picture: this end
/// can forget a credential, only the server can revoke one.
#[tauri::command]
fn unlink(app: tauri::AppHandle) -> Result<(), String> {
    let dir = identity_dir(&app).ok_or_else(|| "no config directory".to_string())?;

    identity::forget(&dir).map_err(failure)
}

/// The pairing in flight, if any: its poll key and the name it offered.
#[derive(Default)]
struct PendingPairing(std::sync::Mutex<Option<(String, String)>>);

/// The port already being taken is the one failure worth naming rather than
/// relaying.
///
/// The operating system's own words arrive in the language Windows was
/// installed in, which need not be the one the interface is speaking, and they
/// describe a socket rather than what to do about it. Everything else is
/// passed through: an unnamed error read literally beats a guess dressed up as
/// an explanation.
fn failure(error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::AddrInUse {
        return PORT_IN_USE.to_string();
    }

    error.to_string()
}

/// The code the interface translates. A code rather than a sentence, so the
/// wording lives in the catalogues with every other sentence.
const PORT_IN_USE: &str = "port-in-use";

/// Point the game at this app, and answer with the file's path so the screen
/// can name what was touched. It writes one file and nothing else: the launch
/// option is not ours to set, and the interface asks for that paste.
#[tauri::command]
fn configure_game(session_name: String) -> Result<String, String> {
    let path = config::config_path()
        .ok_or_else(|| "no USERPROFILE, so the game's config cannot be found".to_string())?;

    config::write_config(&path, &local_url(), &session_name)
        .map_err(|error| format!("{}: {error}", path.display()))?;

    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn stop_recording(recorder: State<'_, Recorder>) {
    recorder.stop();
}

#[tauri::command]
fn status(recorder: State<'_, Recorder>) -> AppStatus {
    AppStatus {
        version: env!("CARGO_PKG_VERSION"),
        recorder: recorder.status(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The config tells the game where to dial, and the recorder is what has
    /// to be holding that socket when it does. They are the same constant
    /// today and two numbers in two files the moment either is edited by hand.
    #[test]
    fn the_config_sends_the_game_to_the_port_the_recorder_takes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        config::write_config(&path, &local_url(), "test").unwrap();

        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

        assert_eq!(written["servers"][0], format!("ws://{}", recording_addr()));
    }

    /// The one place the two halves of the app agree on a spelling: `status`
    /// is read by `src/state.ts`, which cannot see this struct. A renamed
    /// field leaves the screen quietly blank rather than failing anything.
    #[test]
    fn status_answers_in_the_names_the_interface_reads() {
        let recorder = Recorder::new(std::path::PathBuf::from("sessions"));
        let answer = serde_json::to_value(AppStatus {
            version: "0.1.0",
            recorder: recorder.status(),
        })
        .expect("the status has to survive the journey to the window");

        for field in [
            "version",
            "recording",
            "since",
            "sessionId",
            "frames",
            "lastFrameAt",
            "upstream",
        ] {
            assert!(answer.get(field).is_some(), "no {field} in {answer}");
        }

        for field in ["state", "sent", "pending", "retryInSeconds", "failures"] {
            assert!(
                answer["upstream"].get(field).is_some(),
                "no upstream {field} in {answer}"
            );
        }

        // Off, and saying so, before anything has been asked of it.
        assert_eq!(answer["recording"], false);
        assert_eq!(answer["upstream"]["state"], "off");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // First, as the plugin requires: a second launch has to be turned away
        // before anything else has begun. It raises the window already open
        // rather than starting a rival that cannot bind the port.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        // The updater is the reason the signing key exists. Without these two
        // lines the app is signed, its manifest is published, and nothing ever
        // reads either - which is exactly the state v0.1.0 shipped in.
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Restarting is how an installed update takes effect.
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // Sessions live beside the app's own data, not in Documents: they
            // are the app's files, and a match is worth keeping when the
            // upload could not be made.
            let sessions = app.path().app_local_data_dir()?.join("sessions");
            // Off. Building the recorder does not open anything - only
            // `start_recording` does, and only the interface calls it.
            app.manage(Recorder::new(sessions));
            app.manage(PendingPairing::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            status,
            configure_game,
            linked_as,
            begin_pairing,
            poll_pairing,
            unlink
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
