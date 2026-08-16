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
async fn start_recording(recorder: State<'_, Recorder>) -> Result<u16, String> {
    recorder
        .start(
            SocketAddr::from(([127, 0, 0, 1], LOCAL_PORT)),
            COLLECTOR_URL.map(str::to_string),
        )
        .await
        .map_err(|error| error.to_string())
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
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Sessions live beside the app's own data, not in Documents: they
            // are the app's files, and a match is worth keeping when the
            // upload could not be made.
            let sessions = app.path().app_local_data_dir()?.join("sessions");
            // Off. Building the recorder does not open anything - only
            // `start_recording` does, and only the interface calls it.
            app.manage(Recorder::new(sessions));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
