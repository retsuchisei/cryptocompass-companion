//! Apex's own `config.json`: where it goes and what goes in it.
//!
//! The file is not ours to design. The game reads it, and
//! `scripts/liveapi.ps1` in `apex-guideline` already writes it, so this is the
//! same file from the app - a player who ran the script and a player who ran
//! the app leave the game in the same state.
//!
//! What the app cannot write is the one launch option, `+cl_liveapi_enabled 1`:
//! it lives inside the Steam or EA client's own store, which rewrites the file
//! underneath us. The interface asks for that paste and then waits for a frame
//! rather than claiming it worked.

use std::io;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::json;

/// Where the game looks: `%USERPROFILE%\Saved Games\...`.
///
/// `None` when the variable is not set, which off Windows is every machine.
/// The caller says so rather than writing a file no game will read.
pub fn config_path() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|home| config_path_in(Path::new(&home)))
}

fn config_path_in(home: &Path) -> PathBuf {
    home.join("Saved Games")
        .join("Respawn")
        .join("Apex")
        .join("assets")
        .join("temp")
        .join("live_api")
        .join("config.json")
}

/// Point the game at `server` and name the session, mirroring the PowerShell
/// script's `-On`. Strict JSON, unlike the sample Respawn ships: that one has
/// `//` comments and a trailing comma, which only a lenient parser accepts.
pub fn write_config(path: &Path, server: &str, session_name: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        // The game makes `live_api` the first time it records, so on a machine
        // that never has, none of this path exists.
        std::fs::create_dir_all(dir)?;
    }

    keep_a_copy_unless_it_is_ours(path)?;

    let config = json!({
        "apexRankedManaged": true,
        "settings": {
            "cl_liveapi_use_v2": true,
            "cl_liveapi_use_websocket": true,
            "cl_liveapi_allow_requests": true,
            // False, and the whole stack downstream rests on it: a binary
            // frame is recorded as malformed rather than decoded.
            "cl_liveapi_use_protobuf": false,
            "cl_liveapi_pretty_print_log": false,
            "cl_liveapi_session_name": session_name,
            "cl_liveapi_ws_retry_count": 5,
            "cl_liveapi_ws_retry_time": 5,
            "cl_liveapi_ws_timeout": 300,
            "cl_liveapi_ws_keepalive": 10,
        },
        // One address, on purpose. The list would let the game dial the
        // collector as well, and then the same frame arrives by two routes.
        "servers": [server],
    });

    let mut written = serde_json::to_string_pretty(&config)?;
    written.push('\n');

    std::fs::write(path, written)
}

/// A config that is already there is somebody's setup until it proves to be
/// ours, and then it is kept under a name the game does not read - the same
/// care the PowerShell script takes. Ours is replaced in place, or every press
/// of the button would leave another copy behind.
fn keep_a_copy_unless_it_is_ours(path: &Path) -> io::Result<()> {
    let Ok(existing) = std::fs::read(path) else {
        return Ok(());
    };

    if ours(&existing) {
        return Ok(());
    }

    let backup = path.with_file_name(format!(
        "{}.bak-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        Utc::now().format("%Y%m%dT%H%M%S%.3f")
    ));

    std::fs::rename(path, backup)
}

/// Ours if it says so. Anything that does not parse is somebody's - Respawn's
/// own sample does not parse - and is kept rather than judged.
fn ours(existing: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(existing)
        .is_ok_and(|parsed| parsed["apexRankedManaged"] == true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_json(path: &Path) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(path).expect("no config was written"))
            .expect("the game reads strict JSON, so what we write has to parse")
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_config_matches_what_the_powershell_script_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        write_config(&path, "ws://127.0.0.1:7777", "whitepepe").unwrap();

        let parsed = read_json(&path);
        assert_eq!(parsed["apexRankedManaged"], true);
        assert_eq!(parsed["settings"]["cl_liveapi_use_protobuf"], false);
        assert_eq!(parsed["settings"]["cl_liveapi_session_name"], "whitepepe");
        assert_eq!(parsed["servers"][0], "ws://127.0.0.1:7777");

        // The rest of what `scripts/liveapi.ps1` writes. The game takes its
        // whole configuration from this file, so a setting missing here is a
        // setting the game defaults on its own.
        assert_eq!(parsed["settings"]["cl_liveapi_use_v2"], true);
        assert_eq!(parsed["settings"]["cl_liveapi_use_websocket"], true);
        assert_eq!(parsed["settings"]["cl_liveapi_allow_requests"], true);
        assert_eq!(parsed["settings"]["cl_liveapi_pretty_print_log"], false);
        assert_eq!(parsed["settings"]["cl_liveapi_ws_retry_count"], 5);
        assert_eq!(parsed["settings"]["cl_liveapi_ws_retry_time"], 5);
        assert_eq!(parsed["settings"]["cl_liveapi_ws_timeout"], 300);
        assert_eq!(parsed["settings"]["cl_liveapi_ws_keepalive"], 10);
        assert_eq!(
            parsed["servers"].as_array().map(Vec::len),
            Some(1),
            "one address, so a frame cannot arrive by two routes"
        );
    }

    #[test]
    fn somebody_elses_config_is_kept_before_it_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let theirs = r#"{"settings":{"cl_liveapi_session_name":"theirs"}}"#;
        std::fs::write(&path, theirs).unwrap();

        write_config(&path, "ws://127.0.0.1:7777", "ours").unwrap();

        assert_eq!(read_json(&path)["apexRankedManaged"], true);

        let kept: Vec<String> = files_in(dir.path())
            .into_iter()
            .filter(|name| name.starts_with("config.json.bak-"))
            .collect();
        assert_eq!(kept.len(), 1, "{:?}", files_in(dir.path()));
        assert_eq!(
            std::fs::read_to_string(dir.path().join(&kept[0])).unwrap(),
            theirs,
            "a configuration we did not write is not ours to discard"
        );
    }

    #[test]
    fn a_config_that_does_not_parse_is_kept_too() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        // Respawn's own sample has comments and a trailing comma, and whoever
        // edited it by hand meant something by it.
        std::fs::write(&path, "// mine\n{ \"servers\": [], }").unwrap();

        write_config(&path, "ws://127.0.0.1:7777", "ours").unwrap();

        assert!(
            files_in(dir.path())
                .iter()
                .any(|name| name.starts_with("config.json.bak-")),
            "{:?}",
            files_in(dir.path())
        );
    }

    #[test]
    fn our_own_config_is_replaced_without_leaving_copies() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");

        write_config(&path, "ws://127.0.0.1:7777", "first").unwrap();
        write_config(&path, "ws://127.0.0.1:7777", "second").unwrap();
        write_config(&path, "ws://127.0.0.1:7777", "third").unwrap();

        assert_eq!(
            read_json(&path)["settings"]["cl_liveapi_session_name"],
            "third"
        );
        assert_eq!(
            files_in(dir.path()),
            vec!["config.json"],
            "backing up our own config would leave a copy per press of the button"
        );
    }

    #[test]
    fn the_directory_is_made_when_the_game_has_never_written_one() {
        let dir = tempfile::tempdir().unwrap();
        // The game creates `live_api` the first time it records, so on a
        // machine that never has, nothing on this path exists.
        let path = dir.path().join("assets/temp/live_api/config.json");

        write_config(&path, "ws://127.0.0.1:7777", "whitepepe").unwrap();

        assert_eq!(read_json(&path)["apexRankedManaged"], true);
    }

    #[test]
    fn the_config_lives_where_the_game_looks_for_it() {
        let path = config_path_in(Path::new("C:\\Users\\player"));

        assert!(
            path.ends_with("Saved Games/Respawn/Apex/assets/temp/live_api/config.json"),
            "{}",
            path.display()
        );
        assert!(path.starts_with("C:\\Users\\player"), "{}", path.display());
    }
}
