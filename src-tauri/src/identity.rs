//! Who this install is, and how it came to be that.
//!
//! The exchange happens here rather than in the page. If the webview did the
//! pairing, the site's API would need CORS for `tauri://localhost` - widening a
//! public surface for our own convenience - and the token would pass through
//! JavaScript on its way to Rust, which is the one place it has no business
//! being. Nothing in the interface ever sees it.
//!
//! The pairing itself is two values, and they are not interchangeable. The six
//! characters are shown so a person can compare them with what the page shows;
//! the poll key is kept here and is what actually fetches the token. If the
//! short code did both, grinding six characters for five minutes would be a way
//! to collect somebody else's install token.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Where pairing happens. Not a secret - unlike the collector's address - so it
/// has a default and a build made from a clone can still link itself.
pub fn site() -> String {
    option_env!("CRYPTOCOMPASS_SITE_URL")
        .unwrap_or("https://cryptocompass.tech")
        .trim_end_matches('/')
        .to_string()
}

const TIMEOUT: Duration = Duration::from_secs(15);

/// What is kept between runs. The token, and the name this machine offered -
/// held only so the interface can say which install this is without asking the
/// server, which it cannot do while offline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Stored {
    pub token: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pairing {
    /// Shown, and only shown. Never sent anywhere by this app.
    pub code: String,
    /// The page a person is meant to open, with the code already in it.
    pub url: String,
    /// Kept by the caller for the poll. Not shown, not logged.
    #[serde(skip)]
    pub poll_key: String,
}

#[derive(Deserialize)]
struct Opened {
    #[serde(rename = "pollKey")]
    poll_key: String,
    code: String,
    url: String,
}

#[derive(Deserialize)]
struct Polled {
    state: String,
    token: Option<String>,
}

pub fn file(dir: &Path) -> PathBuf {
    dir.join("identity.json")
}

pub fn load(dir: &Path) -> Option<Stored> {
    let raw = fs::read_to_string(file(dir)).ok()?;

    serde_json::from_str(&raw).ok()
}

pub fn save(dir: &Path, stored: &Stored) -> io::Result<()> {
    fs::create_dir_all(dir)?;

    let path = file(dir);
    fs::write(&path, serde_json::to_string_pretty(stored)?)?;

    // Readable by its owner and nobody else. A token is a credential, and a
    // shared machine at a tournament is exactly where this matters.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

pub fn forget(dir: &Path) -> io::Result<()> {
    match fs::remove_file(file(dir)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

fn client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder().timeout(TIMEOUT).build()
}

/// Ask for a pairing. Needs no account: having one is what it is asking for.
pub async fn begin(site: &str, name: &str) -> Result<Pairing, String> {
    let opened: Opened = client()
        .map_err(|error| error.to_string())?
        .post(format!("{site}/api/pair"))
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| error.to_string())?;

    Ok(Pairing {
        code: opened.code,
        url: opened.url,
        poll_key: opened.poll_key,
    })
}

/// Ask whether somebody has confirmed yet.
///
/// `Ok(None)` is "not yet" and is the ordinary answer. The token comes back
/// exactly once, so a caller that drops it has to start a new pairing.
pub async fn collect(site: &str, poll_key: &str) -> Result<Option<String>, String> {
    let response = client()
        .map_err(|error| error.to_string())?
        .get(format!("{site}/api/pair"))
        .bearer_auth(poll_key)
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("gone".to_string());
    }

    let polled: Polled = response
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| error.to_string())?;

    if polled.state == "linked" {
        return polled.token.map(Some).ok_or_else(|| "gone".to_string());
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_stored_is_nobody() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(load(dir.path()), None);
    }

    #[test]
    fn what_is_saved_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let stored = Stored {
            token: "a token".to_string(),
            name: "the tournament laptop".to_string(),
        };

        save(dir.path(), &stored).unwrap();

        assert_eq!(load(dir.path()), Some(stored));
    }

    #[test]
    fn forgetting_twice_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        save(
            dir.path(),
            &Stored {
                token: "a token".to_string(),
                name: "here".to_string(),
            },
        )
        .unwrap();

        forget(dir.path()).unwrap();
        forget(dir.path()).unwrap();

        assert_eq!(load(dir.path()), None);
    }

    /// A file somebody else on the machine can read is a token somebody else
    /// on the machine has.
    #[cfg(unix)]
    #[test]
    fn the_token_is_not_world_readable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        save(
            dir.path(),
            &Stored {
                token: "a token".to_string(),
                name: "here".to_string(),
            },
        )
        .unwrap();

        let mode = fs::metadata(file(dir.path())).unwrap().permissions().mode();

        assert_eq!(mode & 0o077, 0, "group and others must have nothing");
    }

    /// Rubbish on disk is nobody rather than a crash: the file is editable by
    /// hand and a half-written one must not stop the app from starting.
    #[test]
    fn a_file_that_is_not_ours_is_nobody() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path()).unwrap();
        fs::write(file(dir.path()), "{ not json").unwrap();

        assert_eq!(load(dir.path()), None);
    }

    #[test]
    fn the_site_has_no_trailing_slash_to_double_up() {
        assert!(!site().ends_with('/'));
        assert!(site().starts_with("https://"));
    }
}
