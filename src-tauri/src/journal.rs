//! Durable opaque frames, scoped to the original install, retained after ACK.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_BATCH_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_BATCH_FRAMES: usize = 100;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn scope(token: Option<&str>) -> String {
    token
        .map(|value| format!("{:x}", Sha256::digest(value.as_bytes())))
        .unwrap_or_else(|| "unlinked".into())
}

/// Reservations are conservative after a partial write; restart recounts disk.
pub struct Budget {
    max_bytes: u64,
    used_bytes: Mutex<u64>,
}

impl Budget {
    pub fn open(root: &Path, max_bytes: u64) -> io::Result<Arc<Self>> {
        if max_bytes == 0 {
            return Err(invalid("Invalid journal capacity"));
        }
        private_dir(root)?;
        fn bytes(path: &Path) -> io::Result<u64> {
            let mut sum = 0u64;
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let meta = entry.file_type()?;
                if meta.is_symlink() {
                    return Err(invalid("Symlink in journal storage"));
                }
                let size = if meta.is_dir() {
                    bytes(&entry.path())?
                } else {
                    entry.metadata()?.len()
                };
                sum = sum
                    .checked_add(size)
                    .ok_or_else(|| invalid("Journal size overflow"))?;
            }
            Ok(sum)
        }
        Ok(Arc::new(Self {
            max_bytes,
            used_bytes: Mutex::new(bytes(root)?),
        }))
    }

    fn reserve(&self, count: u64) -> io::Result<()> {
        let mut used = self.used_bytes.lock().unwrap();
        let next = used
            .checked_add(count)
            .ok_or_else(|| invalid("Journal capacity overflow"))?;
        if next > self.max_bytes {
            return Err(io::Error::other("Local journal capacity reached"));
        }
        *used = next;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Binary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub seq: i64,
    pub captured_at: String,
    pub kind: Kind,
    pub data: Vec<u8>,
    pub sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Header {
    format: String,
    scope_digest: String,
    client_session_id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireFrame {
    seq: i64,
    captured_at: String,
    #[serde(rename = "type")]
    kind: Kind,
    data: String,
    sha256: String,
}

impl Frame {
    pub fn wire(&self) -> serde_json::Value {
        serde_json::to_value(WireFrame {
            seq: self.seq,
            captured_at: self.captured_at.clone(),
            kind: self.kind,
            data: STANDARD.encode(&self.data),
            sha256: self.sha256.clone(),
        })
        .unwrap()
    }

    fn decode(raw: &[u8], expected: i64) -> io::Result<Self> {
        let wire: WireFrame =
            serde_json::from_slice(raw).map_err(|_| invalid("Invalid journal frame"))?;
        let data = STANDARD
            .decode(&wire.data)
            .map_err(|_| invalid("Invalid journal bytes"))?;
        if wire.seq != expected
            || data.len() > MAX_CAPTURE_BYTES
            || STANDARD.encode(&data) != wire.data
            || format!("{:x}", Sha256::digest(&data)) != wire.sha256
        {
            return Err(invalid("Journal frame identity mismatch"));
        }
        if wire.kind == Kind::Text && std::str::from_utf8(&data).is_err() {
            return Err(invalid("Invalid UTF-8 text frame"));
        }
        Ok(Self {
            seq: wire.seq,
            captured_at: wire.captured_at,
            kind: wire.kind,
            data,
            sha256: wire.sha256,
        })
    }
}

struct State {
    writer: Option<File>,
    last_seq: i64,
    valid_len: u64,
    torn_tail: bool,
}

pub struct Journal {
    path: PathBuf,
    header: Header,
    data_start: u64,
    budget: Arc<Budget>,
    state: Mutex<State>,
}

impl Journal {
    pub fn create(root: &Path, token: Option<&str>, budget: Arc<Budget>) -> io::Result<Self> {
        let folder = root.join("journal-v2").join(scope(token));
        private_dir(&folder)?;
        let header = Header {
            format: "placement-companion-journal-v2".into(),
            scope_digest: scope(token),
            client_session_id: Uuid::new_v4().to_string(),
        };
        let raw = serde_json::to_vec(&header).map_err(|_| invalid("Invalid journal header"))?;
        let pending = folder.join(format!(".{}.pending", header.client_session_id));
        let path = folder.join(format!("{}.journal", header.client_session_id));
        budget.reserve((raw.len() + 40) as u64)?;
        {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&pending)?;
            write_record(&mut file, &raw)?;
            file.sync_all()?;
        }
        install(&pending, &path)?;
        let writer = OpenOptions::new().append(true).read(true).open(&path)?;
        let data_start = writer.metadata()?.len();
        Ok(Self {
            path,
            header,
            data_start,
            budget,
            state: Mutex::new(State {
                writer: Some(writer),
                last_seq: -1,
                valid_len: data_start,
                torn_tail: false,
            }),
        })
    }

    /// Old recordings are read-only: reconnect reuses their ID and exact bytes.
    pub fn open(path: &Path, token: Option<&str>, budget: Arc<Budget>) -> io::Result<Self> {
        if !fs::symlink_metadata(path)?.file_type().is_file() {
            return Err(invalid("Invalid journal path"));
        }
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let total = file.metadata()?.len();
        let raw =
            read_record(&mut file, total)?.ok_or_else(|| invalid("Missing journal header"))?;
        let header: Header =
            serde_json::from_slice(&raw).map_err(|_| invalid("Invalid journal header"))?;
        if header.format != "placement-companion-journal-v2"
            || header.scope_digest != scope(token)
            || Uuid::parse_str(&header.client_session_id).is_err()
        {
            return Err(invalid("Journal belongs to another install"));
        }
        let data_start = file.stream_position()?;
        let mut valid_len = data_start;
        let mut last_seq = -1;
        let mut torn_tail = false;
        loop {
            match read_record(&mut file, total) {
                Ok(Some(raw)) => {
                    Frame::decode(&raw, last_seq + 1)?;
                    last_seq += 1;
                    valid_len = file.stream_position()?;
                }
                Ok(None) => break,
                Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                    torn_tail = true;
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        // A complete record found after process interruption must be durable
        // before it can be offered for upload. Never truncate a partial tail.
        file.sync_all()?;
        Ok(Self {
            path: path.to_path_buf(),
            header,
            data_start,
            budget,
            state: Mutex::new(State {
                writer: None,
                last_seq,
                valid_len,
                torn_tail,
            }),
        })
    }

    pub fn session_id(&self) -> &str {
        &self.header.client_session_id
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn last_seq(&self) -> i64 {
        self.state.lock().unwrap().last_seq
    }
    pub fn has_torn_tail(&self) -> bool {
        self.state.lock().unwrap().torn_tail
    }

    /// A server ownership change cannot silently reassign retained old bytes.
    pub fn bind_server(&self, server_session_id: &str) -> io::Result<()> {
        if Uuid::parse_str(server_session_id)
            .map(|id| id.to_string())
            .ok()
            .as_deref()
            != Some(server_session_id)
        {
            return Err(invalid("Invalid server session identity"));
        }
        let _guard = self.state.lock().unwrap();
        let path = self.path.with_extension("peer");
        let raw = serde_json::to_vec(&serde_json::json!({"format":"placement-journal-peer-v2",
            "clientSessionId":self.session_id(),"serverSessionId":server_session_id}))
        .unwrap();
        if path.exists() {
            if !fs::symlink_metadata(&path)?.file_type().is_file()
                || fs::metadata(&path)?.len() > 4096
                || fs::read(&path)? != raw
            {
                return Err(invalid("Server session ownership changed"));
            }
            return Ok(());
        }
        self.budget.reserve(raw.len() as u64)?;
        let pending = self
            .path
            .with_extension(format!("peer-{}.pending", Uuid::new_v4()));
        {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&pending)?;
            file.write_all(&raw)?;
            file.sync_all()?;
        }
        install(&pending, &path)
    }

    pub fn append(&self, kind: Kind, data: &[u8]) -> io::Result<i64> {
        if data.len() > MAX_CAPTURE_BYTES {
            return Err(invalid("Capture frame exceeds local limit"));
        }
        if kind == Kind::Text && std::str::from_utf8(data).is_err() {
            return Err(invalid("Invalid text frame"));
        }
        let mut state = self.state.lock().unwrap();
        let seq = state
            .last_seq
            .checked_add(1)
            .filter(|v| *v < i64::MAX)
            .ok_or_else(|| invalid("Journal sequence exhausted"))?;
        if state.writer.is_none() || state.torn_tail {
            return Err(invalid("Journal is read-only"));
        }
        let frame = Frame {
            seq,
            captured_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            kind,
            data: data.to_vec(),
            sha256: format!("{:x}", Sha256::digest(data)),
        };
        let raw =
            serde_json::to_vec(&frame.wire()).map_err(|_| invalid("Invalid frame metadata"))?;
        self.budget.reserve((raw.len() + 40) as u64)?;
        let writer = state.writer.as_mut().unwrap();
        if let Err(error) = write_record(writer, &raw).and_then(|()| writer.sync_all()) {
            state.torn_tail = true;
            return Err(error);
        }
        state.valid_len += (raw.len() + 40) as u64;
        state.last_seq = seq;
        Ok(seq)
    }

    pub fn reader(&self, from_seq: i64) -> io::Result<Reader<'_>> {
        if from_seq < 0 || from_seq > self.last_seq() + 1 {
            return Err(invalid("Invalid journal cursor"));
        }
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(self.data_start))?;
        let mut expected = 0;
        while expected < from_seq {
            let raw = read_record(&mut file, self.state.lock().unwrap().valid_len)?
                .ok_or_else(|| invalid("Missing journal sequence"))?;
            Frame::decode(&raw, expected)?;
            expected += 1;
        }
        Ok(Reader {
            journal: self,
            file,
            expected,
            failed: false,
        })
    }
}

pub struct Reader<'a> {
    journal: &'a Journal,
    file: File,
    expected: i64,
    failed: bool,
}

impl Reader<'_> {
    pub fn batch(&mut self) -> io::Result<Vec<Frame>> {
        if self.failed {
            return Err(invalid("Journal reader failed"));
        }
        let result = self.batch_inner();
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn batch_inner(&mut self) -> io::Result<Vec<Frame>> {
        let end = self.journal.state.lock().unwrap().valid_len;
        let mut frames = Vec::new();
        let mut size = 0;
        while frames.len() < MAX_BATCH_FRAMES {
            let before = self.file.stream_position()?;
            let Some(raw) = read_record(&mut self.file, end)? else {
                break;
            };
            let frame = Frame::decode(&raw, self.expected)?;
            if frame.data.len() > MAX_FRAME_BYTES {
                return Err(invalid("Retained frame exceeds upload limit"));
            }
            if size + frame.data.len() > MAX_BATCH_BYTES {
                self.file.seek(SeekFrom::Start(before))?;
                break;
            }
            size += frame.data.len();
            frames.push(frame);
            self.expected += 1;
        }
        Ok(frames)
    }
}

fn write_record(file: &mut File, raw: &[u8]) -> io::Result<()> {
    let len = u32::try_from(raw.len())
        .map_err(|_| invalid("Journal record too large"))?
        .to_le_bytes();
    file.write_all(&len)?;
    file.write_all(raw)?;
    file.write_all(&Sha256::digest(raw))?;
    file.write_all(&len)
}

fn read_record(file: &mut File, end: u64) -> io::Result<Option<Vec<u8>>> {
    let position = file.stream_position()?;
    if position == end {
        return Ok(None);
    }
    let torn = || io::Error::new(io::ErrorKind::UnexpectedEof, "Incomplete journal tail");
    if end.saturating_sub(position) < 4 {
        return Err(torn());
    }
    let mut prefix = [0; 4];
    file.read_exact(&mut prefix)?;
    let len = u32::from_le_bytes(prefix) as usize;
    if len > MAX_RECORD_BYTES {
        return Err(invalid("Invalid journal record length"));
    }
    if end - position < (len + 40) as u64 {
        return Err(torn());
    }
    let mut raw = vec![0; len];
    file.read_exact(&mut raw)?;
    let mut checksum = [0; 32];
    file.read_exact(&mut checksum)?;
    let mut suffix = [0; 4];
    file.read_exact(&mut suffix)?;
    if prefix != suffix || Sha256::digest(&raw).as_slice() != checksum {
        return Err(invalid("Journal record checksum mismatch"));
    }
    Ok(Some(raw))
}

fn private_dir(path: &Path) -> io::Result<()> {
    if path.exists() {
        if !fs::symlink_metadata(path)?.file_type().is_dir() {
            return Err(invalid("Invalid journal directory"));
        }
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Invalid journal root"))?;
    private_dir(parent)?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn install(pending: &Path, path: &Path) -> io::Result<()> {
    fs::hard_link(pending, path)?;
    File::open(path.parent().unwrap())?.sync_all()?;
    // Only the staging name is removed; the same durable journal inode remains.
    fs::remove_file(pending)?;
    File::open(path.parent().unwrap())?.sync_all()
}

#[cfg(windows)]
fn install(pending: &Path, path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
    let from: Vec<u16> = pending.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // Both pointers are owned, NUL-terminated UTF-16 buffers for this call.
    // No REPLACE_EXISTING: an owned journal must never overwrite another file.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::Write;

    #[test]
    fn exact_text_binary_scope_and_session_survive_a_process_restart() {
        let dir = tempfile::tempdir().unwrap();
        let budget = Budget::open(dir.path(), 8 * 1024 * 1024).unwrap();
        let journal =
            Journal::create(dir.path(), Some("original-install"), budget.clone()).unwrap();
        let id = journal.session_id().to_string();
        assert_eq!(journal.append(Kind::Text, b"{ \"future\": 1 }").unwrap(), 0);
        assert_eq!(journal.append(Kind::Binary, &[0, 255]).unwrap(), 1);
        let path = journal.path().to_path_buf();
        drop(journal);
        let recovered = Journal::open(&path, Some("original-install"), budget).unwrap();
        assert_eq!(recovered.session_id(), id);
        assert_eq!(recovered.last_seq(), 1);
        let frames = recovered.reader(0).unwrap().batch().unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].data, b"{ \"future\": 1 }");
        assert_eq!(frames[1].data, vec![0, 255]);
        assert_eq!(frames[0].seq, 0);
        assert_eq!(frames[1].seq, 1);
    }

    #[test]
    fn a_peer_identity_binding_survives_restart_and_cannot_be_reassigned() {
        let dir = tempfile::tempdir().unwrap();
        let budget = Budget::open(dir.path(), 1024 * 1024).unwrap();
        let journal = Journal::create(dir.path(), Some("install"), budget.clone()).unwrap();
        let first = "11111111-1111-4111-8111-111111111111";
        journal.bind_server(first).unwrap();
        journal.bind_server(first).unwrap();
        let path = journal.path().to_path_buf();
        drop(journal);
        let recovered = Journal::open(&path, Some("install"), budget).unwrap();
        recovered.bind_server(first).unwrap();
        assert!(recovered
            .bind_server("22222222-2222-4222-8222-222222222222")
            .is_err());
    }

    #[test]
    fn relinking_cannot_reassign_an_old_journal_to_the_new_token() {
        let dir = tempfile::tempdir().unwrap();
        let budget = Budget::open(dir.path(), 8 * 1024 * 1024).unwrap();
        let original =
            Journal::create(dir.path(), Some("original-install"), budget.clone()).unwrap();
        original.append(Kind::Text, b"private old owner").unwrap();
        assert!(Journal::open(original.path(), Some("new-install"), budget.clone()).is_err());
        assert!(Journal::open(original.path(), None, budget).is_err());
        assert_ne!(scope(Some("original-install")), scope(Some("new-install")));
    }

    #[test]
    fn a_torn_tail_keeps_all_complete_frames_and_original_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let budget = Budget::open(dir.path(), 8 * 1024 * 1024).unwrap();
        let journal = Journal::create(dir.path(), Some("install"), budget.clone()).unwrap();
        journal.append(Kind::Text, b"complete").unwrap();
        let path = journal.path().to_path_buf();
        drop(journal);
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&[10, 0, 0])
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        let recovered = Journal::open(&path, Some("install"), budget).unwrap();
        assert!(recovered.has_torn_tail());
        assert_eq!(recovered.last_seq(), 0);
        assert_eq!(
            recovered.reader(0).unwrap().batch().unwrap()[0].data,
            b"complete"
        );
        assert!(recovered
            .append(Kind::Text, b"cannot append over a torn tail")
            .is_err());
        assert_eq!(std::fs::read(path).unwrap(), before);
    }

    #[test]
    fn the_active_window_is_bounded_by_decoded_bytes_and_frame_count() {
        let dir = tempfile::tempdir().unwrap();
        let budget = Budget::open(dir.path(), 16 * 1024 * 1024).unwrap();
        let journal = Journal::create(dir.path(), Some("install"), budget).unwrap();
        for _ in 0..3 {
            journal
                .append(Kind::Binary, &vec![42; 1024 * 1024])
                .unwrap();
        }
        let mut reader = journal.reader(0).unwrap();
        assert_eq!(reader.batch().unwrap().len(), 2);
        assert_eq!(reader.batch().unwrap().len(), 1);
        assert!(reader.batch().unwrap().is_empty());
        for _ in 0..102 {
            journal.append(Kind::Text, b"small").unwrap();
        }
        assert_eq!(reader.batch().unwrap().len(), 100);
        assert_eq!(reader.batch().unwrap().len(), 2);
    }

    #[test]
    fn capacity_backpressure_preserves_the_prior_complete_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let budget = Budget::open(dir.path(), 2048).unwrap();
        let journal = Journal::create(dir.path(), Some("install"), budget).unwrap();
        journal.append(Kind::Text, b"owned").unwrap();
        let before = std::fs::read(journal.path()).unwrap();
        assert!(journal.append(Kind::Binary, &vec![0; 4096]).is_err());
        assert_eq!(journal.last_seq(), 0);
        assert_eq!(std::fs::read(journal.path()).unwrap(), before);
    }

    #[test]
    fn unsupported_upload_size_is_retained_instead_of_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let budget = Budget::open(dir.path(), 8 * 1024 * 1024).unwrap();
        let journal = Journal::create(dir.path(), None, budget).unwrap();
        journal
            .append(Kind::Binary, &vec![0; 1024 * 1024 + 1])
            .unwrap();
        assert_eq!(journal.last_seq(), 0);
        assert!(journal.reader(0).unwrap().batch().is_err());
        assert!(journal.path().exists());
    }
}
