//! The session log: every frame the game sent, in the order it sent them.
//!
//! The format is not ours to invent. It is `services/liveapi/session.py` in
//! `apex-guideline`, so that `services/liveapi/replay.py` verifies this
//! recorder unmodified - one file per connection, a header line, one line per
//! frame, gzipped on close. `seq` is the one addition, and it is additive:
//! replay.py reads `at` and `event` and ignores what it does not know.
//!
//! Nothing is filtered here and nothing is interpreted beyond `category`,
//! which decides whether the first frame is the header's Init. A match cannot
//! be replayed to collect what a filter dropped.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::{json, Value};

/// Written into every header, so a log says which program wrote it. The
/// server's own collector says `liveapi-collector/1`.
const COLLECTOR_VERSION: &str = "cryptocompass-companion/1";

/// Text that did not parse is kept, but only as much of it as says what went
/// wrong. session.py's limit, in characters.
const RAW_LIMIT: usize = 4096;

/// Append-only JSONL for one connection.
pub struct SessionLog {
    session_id: String,
    path: PathBuf,
    /// `None` until the first frame. A probe that opens a socket and leaves
    /// should not litter the directory with empty logs.
    handle: Option<File>,
    /// Rows written so far, and therefore the next `seq`: a row's number is
    /// how many rows came before it. The header is not a row and takes none.
    count: u64,
}

impl SessionLog {
    /// Name the file and make sure the directory can hold it. The file itself
    /// waits for the first frame.
    pub fn create(root: &Path, session_id: &str) -> io::Result<Self> {
        std::fs::create_dir_all(root)?;

        Ok(Self {
            session_id: session_id.to_string(),
            path: root.join(format!("{session_id}.jsonl")),
            handle: None,
            count: 0,
        })
    }

    /// Record one text frame. The first one, if it is Init, becomes the
    /// header instead of a row.
    pub fn write_text(&mut self, raw: &str) -> io::Result<()> {
        let Ok(event) = serde_json::from_str::<Value>(raw) else {
            return self.write_malformed(raw);
        };

        if self.handle.is_none() && is_init(&event) {
            return self.open(Some(event));
        }

        self.open_if_needed()?;
        let row = json!({"at": now(), "seq": self.count, "event": event});
        self.emit(row)?;
        self.count += 1;
        Ok(())
    }

    /// Record a binary frame, which is a frame we cannot read: the config the
    /// app writes sets `cl_liveapi_use_protobuf` to false, so one arriving
    /// means the game is not configured the way the rest of the stack
    /// assumes. Its length says that much without decoding it.
    pub fn write_binary(&mut self, len: usize) -> io::Result<()> {
        self.write_malformed(&format!("binary frame, {len} bytes"))
    }

    /// Rows written, which is what the interface counts. The header is not a
    /// row, so an Init-only session counts zero.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Close the file and compress it, returning the path it ended up at.
    ///
    /// The moment is unambiguous: a session log is appended to only while its
    /// connection lives and is finished the instant it ends. JSONL of
    /// repeating keys compresses by roughly ten times, and `.jsonl.gz` is
    /// what replay.py looks for.
    ///
    /// A session that recorded nothing has no file, and says so with a
    /// message naming itself rather than letting the caller log the
    /// filesystem's "no such file".
    pub fn close(mut self) -> io::Result<PathBuf> {
        let Some(handle) = self.handle.take() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {} recorded no frames", self.session_id),
            ));
        };

        drop(handle);
        let packed = self.path.with_extension("jsonl.gz");

        {
            let mut plain = File::open(&self.path)?;
            let mut squeezed = GzEncoder::new(File::create(&packed)?, Compression::default());
            io::copy(&mut plain, &mut squeezed)?;
            // Explicit, because a dropped encoder swallows the error from
            // writing the trailer and leaves a truncated archive.
            squeezed.finish()?;
        }

        // After the read handle is gone: Windows refuses to remove an open
        // file, and this runs on Windows.
        std::fs::remove_file(&self.path)?;
        Ok(packed)
    }

    fn write_malformed(&mut self, raw: &str) -> io::Result<()> {
        self.open_if_needed()?;
        let clipped: String = raw.chars().take(RAW_LIMIT).collect();
        let row = json!({"at": now(), "seq": self.count, "malformed": true, "raw": clipped});
        self.emit(row)?;
        self.count += 1;
        Ok(())
    }

    fn open_if_needed(&mut self) -> io::Result<()> {
        if self.handle.is_none() {
            return self.open(None);
        }

        Ok(())
    }

    fn open(&mut self, init: Option<Value>) -> io::Result<()> {
        self.handle = Some(File::create(&self.path)?);
        self.emit(json!({
            "sessionId": self.session_id,
            "openedAt": now(),
            "collectorVersion": COLLECTOR_VERSION,
            "init": init,
        }))
    }

    fn emit(&mut self, row: Value) -> io::Result<()> {
        let handle = self
            .handle
            .as_mut()
            .expect("emit is only reached once the file is open");

        // A bare File and no BufWriter, on purpose: every row reaches the
        // disk as it is written. A match cannot be replayed, so an event
        // buffered and lost to a crash is lost for good, and the rate is a
        // few frames a second.
        writeln!(handle, "{row}")
    }
}

/// The whole of the interpretation this app is allowed. session.py compares
/// the same field the same way, case and all.
fn is_init(event: &Value) -> bool {
    event
        .get("category")
        .and_then(Value::as_str)
        .is_some_and(|category| category.eq_ignore_ascii_case("init"))
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use std::io::Read;
    use std::path::Path;

    fn read_gzip_lines(path: &Path) -> Vec<String> {
        let mut text = String::new();
        GzDecoder::new(std::fs::File::open(path).unwrap())
            .read_to_string(&mut text)
            .unwrap();
        text.lines().map(str::to_string).collect()
    }

    #[test]
    fn the_first_init_becomes_the_header_and_events_are_numbered() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = SessionLog::create(dir.path(), "s1").unwrap();

        log.write_text(r#"{"category":"init","name":"replay"}"#)
            .unwrap();
        log.write_text(r#"{"category":"matchSetup","map":"m"}"#)
            .unwrap();
        log.write_text("not json at all").unwrap();

        let path = log.close().unwrap();
        assert!(path.to_string_lossy().ends_with(".jsonl.gz"));

        let lines = read_gzip_lines(&path);
        let header: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(header["sessionId"], "s1");
        assert_eq!(header["init"]["name"], "replay");

        let first: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(first["seq"], 0);
        assert_eq!(first["event"]["category"], "matchSetup");
        assert!(first["at"].is_string());

        let second: serde_json::Value = serde_json::from_str(&lines[2]).unwrap();
        assert_eq!(second["seq"], 1);
        assert_eq!(second["malformed"], true);
        assert_eq!(second["raw"], "not json at all");
    }

    #[test]
    fn a_frame_keeps_the_field_order_the_game_sent() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = SessionLog::create(dir.path(), "s6").unwrap();
        let frame = r#"{"timestamp":3,"category":"ringStartClosing","endRadius":31000.0}"#;
        log.write_text(frame).unwrap();

        // The order is the game's, not ours. A relay that sorts the fields
        // has already started editing the thing it promised to carry.
        let lines = read_gzip_lines(&log.close().unwrap());
        assert!(
            lines[1].contains(&format!(r#""event":{frame}"#)),
            "fields were reordered: {}",
            lines[1]
        );
    }

    #[test]
    fn a_binary_frame_is_recorded_as_malformed_rather_than_decoded() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = SessionLog::create(dir.path(), "s3").unwrap();

        log.write_text(r#"{"category":"matchSetup"}"#).unwrap();
        log.write_binary(64).unwrap();

        // Rows, not frames the game sent: the header is not a row.
        assert_eq!(log.count(), 2);

        let lines = read_gzip_lines(&log.close().unwrap());
        let row: serde_json::Value = serde_json::from_str(&lines[2]).unwrap();
        assert_eq!(row["seq"], 1);
        assert_eq!(row["malformed"], true);
        assert!(
            row["raw"].as_str().unwrap().contains("64"),
            "the length is the whole of what we can say about it: {row}"
        );
    }

    #[test]
    fn an_init_alone_counts_no_rows() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = SessionLog::create(dir.path(), "s4").unwrap();
        log.write_text(r#"{"category":"init","name":"replay"}"#)
            .unwrap();
        assert_eq!(log.count(), 0);
    }

    #[test]
    fn a_session_that_recorded_nothing_names_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let log = SessionLog::create(dir.path(), "s5").unwrap();

        // A probe that connects and leaves. There is no file to name, and
        // naming one that does not exist would send the caller looking.
        let error = log.close().unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(
            error.to_string().contains("s5"),
            "the caller logs this, so it has to say which session: {error}"
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_session_with_no_init_still_opens() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = SessionLog::create(dir.path(), "s2").unwrap();
        log.write_text(r#"{"category":"matchSetup"}"#).unwrap();
        let path = log.close().unwrap();
        let lines = read_gzip_lines(&path);
        let header: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert!(header["init"].is_null());
    }
}
