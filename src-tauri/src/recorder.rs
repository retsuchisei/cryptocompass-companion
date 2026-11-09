//! Capture is durable before delivery. The recording switch owns every task.

use crate::delivery::{DeliveryState, DurableUpstream};
use crate::journal::{Budget, Journal, Kind};
use crate::listen::{bind, serve, Frame};
use crate::session::SessionLog;
use chrono::Utc;
use serde::Serialize;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub recording: bool,
    pub since: Option<i64>,
    pub session_id: Option<String>,
    pub frames: u64,
    pub last_frame_at: Option<i64>,
    pub upstream: UpstreamReport,
    pub error: Option<&'static str>,
}

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamReport {
    pub state: &'static str,
    pub sent: u64,
    pub confirmed: u64,
    pub pending: usize,
    pub retry_in_seconds: u64,
    pub failures: u32,
}

impl UpstreamReport {
    fn quiet(state: &'static str) -> Self {
        Self {
            state,
            sent: 0,
            confirmed: 0,
            pending: 0,
            retry_in_seconds: 0,
            failures: 0,
        }
    }
    fn of(upstream: &DurableUpstream) -> Self {
        let state = match upstream.state() {
            DeliveryState::Idle => "idle",
            DeliveryState::Connecting => "connecting",
            DeliveryState::Durable => "durable",
            DeliveryState::Legacy => "legacy",
            DeliveryState::Retrying => "retrying",
            DeliveryState::Refused => "refused",
            DeliveryState::Conflict => "conflict",
            DeliveryState::LocalFailure => "local-failure",
            DeliveryState::Incompatible => "incompatible",
        };
        Self {
            state,
            sent: upstream.sent(),
            confirmed: upstream.confirmed(),
            pending: upstream.pending() as usize,
            retry_in_seconds: upstream.retry_seconds(),
            failures: 0,
        }
    }
    fn combine(left: Self, right: Self) -> Self {
        const ORDER: [&str; 13] = [
            "refused",
            "conflict",
            "local-failure",
            "incompatible",
            "retrying",
            "legacy",
            "connecting",
            "checking",
            "durable",
            "idle",
            "paused",
            "unlinked",
            "off",
        ];
        let rank = |state| ORDER.iter().position(|s| *s == state).unwrap_or(0);
        let state = if rank(left.state) <= rank(right.state) {
            left.state
        } else {
            right.state
        };
        Self {
            state,
            sent: left.sent.saturating_add(right.sent),
            confirmed: left.confirmed.saturating_add(right.confirmed),
            pending: left.pending.saturating_add(right.pending),
            retry_in_seconds: left.retry_in_seconds.max(right.retry_in_seconds),
            failures: left.failures.saturating_add(right.failures),
        }
    }
}

struct Backlog {
    task: JoinHandle<()>,
    report: Arc<Mutex<UpstreamReport>>,
}
impl Backlog {
    fn start(current: Arc<Journal>, budget: Arc<Budget>, url: String, token: String) -> Self {
        let report = Arc::new(Mutex::new(UpstreamReport::quiet("checking")));
        let progress = report.clone();
        let task = tokio::spawn(async move {
            let mut completed = UpstreamReport::quiet("idle");
            let folder = current.path().parent().unwrap().to_path_buf();
            let Ok(entries) = std::fs::read_dir(folder) else {
                *progress.lock().unwrap() = UpstreamReport::quiet("local-failure");
                return;
            };
            for entry in entries {
                let Ok(entry) = entry else {
                    completed.state = "local-failure";
                    continue;
                };
                let path = entry.path();
                if path == current.path() || path.extension().is_none_or(|s| s != "journal") {
                    continue;
                }
                let key = token.clone();
                let available = budget.clone();
                let opened = tokio::task::spawn_blocking(move || {
                    Journal::open(&path, Some(&key), available)
                })
                .await;
                let Ok(Ok(old)) = opened else {
                    completed.state = "local-failure";
                    continue;
                };
                if old.has_torn_tail() {
                    completed.state = "local-failure";
                }
                if old.last_seq() < 0 {
                    continue;
                }
                let count = (old.last_seq() + 1) as u64;
                let sender = DurableUpstream::start(url.clone(), token.clone(), Arc::new(old));
                loop {
                    let partial = UpstreamReport::of(&sender);
                    *progress.lock().unwrap() = UpstreamReport::combine(completed, partial);
                    let state = sender.state();
                    let terminal = matches!(
                        state,
                        DeliveryState::Refused
                            | DeliveryState::Conflict
                            | DeliveryState::LocalFailure
                            | DeliveryState::Incompatible
                    );
                    if terminal
                        || (state == DeliveryState::Durable && sender.pending() == 0)
                        || (state == DeliveryState::Legacy && sender.sent() >= count)
                    {
                        completed = UpstreamReport::combine(completed, partial);
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                let refused = sender.state() == DeliveryState::Refused;
                drop(sender);
                if refused {
                    break;
                }
            }
            *progress.lock().unwrap() = completed;
        });
        Self { task, report }
    }
}
impl Drop for Backlog {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub struct Recorder {
    root: PathBuf,
    running: Mutex<Option<Running>>,
    max_journal_bytes: u64,
    last_report: Mutex<UpstreamReport>,
    last_error: Mutex<Option<&'static str>>,
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Running {
    port: u16,
    since: i64,
    task: JoinHandle<()>,
    session: Arc<Session>,
}
struct Session {
    root: PathBuf,
    collector: Option<String>,
    token: Option<String>,
    journal: Arc<Journal>,
    log: Mutex<Option<SessionLog>>,
    upstream: Mutex<Option<DurableUpstream>>,
    backlog: Mutex<Option<Backlog>>,
    counters: Mutex<Counters>,
    error: Mutex<Option<&'static str>>,
    failed: Notify,
    stopped: AtomicBool,
}
#[derive(Default, Clone)]
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
            max_journal_bytes: 8 * 1024 * 1024 * 1024,
            last_report: Mutex::new(UpstreamReport::quiet("off")),
            last_error: Mutex::new(None),
        }
    }
    pub async fn start(
        &self,
        addr: SocketAddr,
        collector: Option<String>,
        token: Option<String>,
    ) -> io::Result<u16> {
        {
            let mut slot = self.running.lock().unwrap();
            if let Some(running) = slot.as_ref() {
                if !running.task.is_finished() && running.session.error.lock().unwrap().is_none() {
                    return Ok(running.port);
                }
            }
            if let Some(old) = slot.take() {
                old.task.abort();
                old.session.finish();
            }
        }
        let listener = bind(addr).await?;
        let port = listener.local_addr()?.port();
        let budget = Budget::open(&self.root, self.max_journal_bytes)
            .map_err(|_| io::Error::other("local-storage-unavailable"))?;
        let journal = Arc::new(
            Journal::create(&self.root, token.as_deref(), budget.clone())
                .map_err(|_| io::Error::other("local-storage-unavailable"))?,
        );
        let session = Arc::new(Session {
            root: self.root.clone(),
            collector,
            token,
            journal,
            log: Mutex::new(None),
            upstream: Mutex::new(None),
            backlog: Mutex::new(None),
            counters: Mutex::new(Counters::default()),
            error: Mutex::new(None),
            failed: Notify::new(),
            stopped: AtomicBool::new(false),
        });
        let mut slot = self.running.lock().unwrap();
        if let Some(existing) = slot.as_ref() {
            return Ok(existing.port);
        }
        if let (Some(url), Some(token)) = (&session.collector, &session.token) {
            *session.backlog.lock().unwrap() = Some(Backlog::start(
                session.journal.clone(),
                budget,
                url.clone(),
                token.clone(),
            ));
        }
        let task_session = session.clone();
        let sink = session.clone();
        let task = tokio::spawn(async move {
            tokio::select! {
                result = serve(listener,move |frame| sink.on_frame(frame)) => {
                    if result.is_err() { task_session.fail("capture-unavailable"); }
                },
                _ = task_session.failed.notified() => {},
            }
            task_session.finish();
        });
        *self.last_error.lock().unwrap() = None;
        *slot = Some(Running {
            port,
            since: now_ms(),
            task,
            session,
        });
        Ok(port)
    }
    pub fn stop(&self) {
        let Some(running) = self.running.lock().unwrap().take() else {
            return;
        };
        let mut report = running.session.upstream_report();
        if report.state != "off" && report.state != "unlinked" {
            report.state = "paused";
        }
        *self.last_report.lock().unwrap() = report;
        // No detached tail drain: both capture and delivery belong to this switch.
        running.task.abort();
        running.session.finish();
        *self.last_error.lock().unwrap() = *running.session.error.lock().unwrap();
    }
    pub fn status(&self) -> Status {
        let slot = self.running.lock().unwrap();
        let Some(running) = slot.as_ref() else {
            return Status {
                recording: false,
                since: None,
                session_id: None,
                frames: 0,
                last_frame_at: None,
                upstream: *self.last_report.lock().unwrap(),
                error: *self.last_error.lock().unwrap(),
            };
        };
        let counters = running.session.counters.lock().unwrap().clone();
        let error = *running.session.error.lock().unwrap();
        Status {
            recording: !running.task.is_finished() && error.is_none(),
            since: Some(running.since),
            session_id: counters.session_id,
            frames: counters.frames,
            last_frame_at: counters.last_frame_at,
            upstream: running.session.upstream_report(),
            error,
        }
    }
}

impl Session {
    fn fail(&self, code: &'static str) {
        self.stopped.store(true, Ordering::Release);
        *self.error.lock().unwrap() = Some(code);
        drop(self.upstream.lock().unwrap().take());
        drop(self.backlog.lock().unwrap().take());
        self.failed.notify_one();
    }
    fn on_frame(&self, frame: Frame) {
        if self.stopped.load(Ordering::Acquire) || self.error.lock().unwrap().is_some() {
            return;
        }
        let result = match frame {
            Frame::Text(text) => {
                self.capture(Kind::Text, text.as_bytes(), |log| log.write_text(&text))
            }
            Frame::Binary(bytes) => {
                self.capture(Kind::Binary, &bytes, |log| log.write_binary(bytes.len()))
            }
            Frame::Closed => {
                self.close_log();
                return;
            }
            Frame::Failure(code) => {
                self.fail(code);
                return;
            }
        };
        if result.is_err() {
            self.fail("local-write-failed");
        }
    }
    fn capture(
        &self,
        kind: Kind,
        bytes: &[u8],
        row: impl FnOnce(&mut SessionLog) -> io::Result<()>,
    ) -> io::Result<()> {
        if self.stopped.load(Ordering::Acquire) {
            return Ok(());
        }
        self.journal.append(kind, bytes)?;
        self.write(row)?;
        {
            let mut counters = self.counters.lock().unwrap();
            counters.frames += 1;
            counters.last_frame_at = Some(now_ms());
        }
        if let (Some(url), Some(token)) = (&self.collector, &self.token) {
            let mut sender = self.upstream.lock().unwrap();
            if !self.stopped.load(Ordering::Acquire) {
                sender
                    .get_or_insert_with(|| {
                        DurableUpstream::start(url.clone(), token.clone(), self.journal.clone())
                    })
                    .wake();
            }
        }
        Ok(())
    }
    fn write(&self, row: impl FnOnce(&mut SessionLog) -> io::Result<()>) -> io::Result<()> {
        let mut log = self.log.lock().unwrap();
        if log.is_none() {
            let id = new_session_id();
            *log = Some(SessionLog::create(&self.root, &id)?);
            self.counters.lock().unwrap().session_id = Some(id);
        }
        row(log.as_mut().unwrap())
    }
    fn finish(&self) {
        self.stopped.store(true, Ordering::Release);
        drop(self.upstream.lock().unwrap().take());
        drop(self.backlog.lock().unwrap().take());
        self.close_log();
    }
    fn close_log(&self) {
        if let Some(log) = self.log.lock().unwrap().take() {
            if log.close().is_err() {
                self.fail("local-write-failed");
            }
        }
    }
    fn upstream_report(&self) -> UpstreamReport {
        let state = if self.collector.is_none() {
            "off"
        } else if self.token.is_none() {
            "unlinked"
        } else {
            "idle"
        };
        let current = self
            .upstream
            .lock()
            .unwrap()
            .as_ref()
            .map(UpstreamReport::of)
            .unwrap_or_else(|| {
                let mut report = UpstreamReport::quiet(state);
                report.pending = (self.journal.last_seq() + 1) as usize;
                report
            });
        let backlog = self
            .backlog
            .lock()
            .unwrap()
            .as_ref()
            .map(|backlog| *backlog.report.lock().unwrap())
            .unwrap_or_else(|| UpstreamReport::quiet("off"));
        UpstreamReport::combine(current, backlog)
    }
}
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
            // This helper checks legacy exports; durable journals are nested.
            .filter(|entry| entry.as_ref().is_ok_and(|entry| entry.path().is_file()))
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
    async fn dropping_the_recorder_stops_its_owned_listener() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new(dir.path().to_path_buf());
        let port = recorder
            .start(anywhere_on_loopback(), None, None)
            .await
            .unwrap();
        drop(recorder);
        assert!(
            eventually_refused(port).await,
            "Recorder destruction detached its active listener"
        );
    }

    #[tokio::test]
    async fn a_late_capture_callback_cannot_restart_writing_after_recording_off() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new(dir.path().to_path_buf());
        recorder
            .start(anywhere_on_loopback(), None, None)
            .await
            .unwrap();
        let session = recorder
            .running
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .session
            .clone();
        recorder.stop();
        let before = session.journal.last_seq();
        session.on_frame(Frame::Text("late callback".into()));
        assert_eq!(
            session.journal.last_seq(),
            before,
            "Recording off admitted a late capture callback"
        );
        assert!(session.upstream.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn stopping_recording_retains_exact_install_scoped_journal_frames() {
        use crate::journal::{scope, Budget, Journal};
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new(dir.path().to_path_buf());
        let port = recorder
            .start(
                anywhere_on_loopback(),
                None,
                Some("original-install".into()),
            )
            .await
            .unwrap();
        let mut game = game_connects(port).await;
        game.send(Message::Text("{ \"future\": 1 }".into()))
            .await
            .unwrap();
        game.send(Message::Binary(vec![0, 255].into()))
            .await
            .unwrap();
        assert!(within(5, || recorder.status().frames == 2).await);
        recorder.stop();
        assert!(eventually_refused(port).await);
        let folder = dir
            .path()
            .join("journal-v2")
            .join(scope(Some("original-install")));
        let path = std::fs::read_dir(folder)
            .unwrap()
            .map(|p| p.unwrap().path())
            .find(|p| p.extension().is_some_and(|s| s == "journal"))
            .unwrap();
        let budget = Budget::open(dir.path(), 8 * 1024 * 1024).unwrap();
        let saved = Journal::open(&path, Some("original-install"), budget).unwrap();
        let frames = saved.reader(0).unwrap().batch().unwrap();
        assert_eq!(frames[0].data, b"{ \"future\": 1 }");
        assert_eq!(frames[1].data, vec![0, 255]);
        assert!(!recorder.status().recording);
    }

    #[tokio::test]
    async fn local_journal_failure_stops_capture_and_never_forwards_the_failed_frame() {
        let dir = tempfile::tempdir().unwrap();
        let mut recorder = Recorder::new(dir.path().to_path_buf());
        recorder.max_journal_bytes = 750;
        let port = recorder
            .start(anywhere_on_loopback(), None, None)
            .await
            .unwrap();
        let mut game = game_connects(port).await;
        game.send(Message::Text("owned".into())).await.unwrap();
        assert!(within(5, || recorder.status().frames == 1).await);
        game.send(Message::Binary(vec![42; 4096].into()))
            .await
            .unwrap();
        assert!(within(5, || !recorder.status().recording).await);
        assert_eq!(recorder.status().error, Some("local-write-failed"));
        assert_eq!(recorder.status().frames, 1);
        assert!(eventually_refused(port).await);
        recorder.stop();
    }

    #[tokio::test]
    async fn export_close_failure_stops_capture_and_survives_recording_off() {
        for close_game in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let recorder = Recorder::new(dir.path().to_path_buf());
            let port = recorder
                .start(anywhere_on_loopback(), None, None)
                .await
                .unwrap();
            let mut game = game_connects(port).await;
            game.send(Message::Text("owned".into())).await.unwrap();
            assert!(within(5, || recorder.status().frames == 1).await);
            let id = recorder.status().session_id.unwrap();
            std::fs::create_dir(dir.path().join(format!("{id}.jsonl.gz"))).unwrap();
            if close_game {
                game.close(None).await.unwrap();
                assert!(within(5, || recorder.status().error.is_some()).await);
                assert!(
                    eventually_refused(port).await,
                    "Export failure left the capture listener alive"
                );
            }
            recorder.stop();
            assert_eq!(
                recorder.status().error,
                Some("local-write-failed"),
                "Recording off hid its file failure"
            );
        }
    }

    #[tokio::test]
    async fn recording_off_cancels_upload_even_after_the_game_disconnected() {
        use serde_json::{json, Value};
        use tokio::sync::Notify;
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new(dir.path().to_path_buf());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", listener.local_addr().unwrap());
        let received = Arc::new(Notify::new());
        let seen = received.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let resume: Value =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            let id = resume["clientSessionId"]
                .as_str()
                .expect("v2 resume must precede source frames");
            ws.send(Message::Text(
                json!({"protocolVersion":2,"type":"ready","clientSessionId":id,
                "serverSessionId":"11111111-1111-4111-8111-111111111111","contiguousSeq":-1,
                "durability":"local-fsync-pg-commit","limits":{"frameBytes":1048576,
                "batchBytes":2097152,"batchFrames":100,"flushMs":250}})
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
            let batch: Value =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(batch["frames"].as_array().unwrap().len(), 1);
            seen.notify_one();
            // Withhold ACK: pending work must remain owned by the recording switch.
            assert!(
                tokio::time::timeout(Duration::from_secs(2), ws.next())
                    .await
                    .is_ok(),
                "Upload survived recording off as a detached drain task"
            );
        });
        let port = recorder
            .start(
                anywhere_on_loopback(),
                Some(url),
                Some("recording-install-token".into()),
            )
            .await
            .unwrap();
        let mut game = game_connects(port).await;
        game.send(Message::Text("opaque".into())).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), received.notified())
            .await
            .unwrap();
        game.close(None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        recorder.stop();
        server.await.unwrap();
        assert!(!recorder.status().recording);
    }

    #[tokio::test]
    async fn saved_backlog_resumes_only_after_explicit_recording_consent_with_the_same_install() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use serde_json::{json, Value};
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new(dir.path().to_path_buf());
        let port = recorder
            .start(
                anywhere_on_loopback(),
                None,
                Some("test-install-token".into()),
            )
            .await
            .unwrap();
        let mut game = game_connects(port).await;
        game.send(Message::Text("saved offline".into()))
            .await
            .unwrap();
        assert!(within(5, || recorder.status().frames == 1).await);
        recorder.stop();
        let collector = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", collector.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = collector.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let resume: Value =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            let id = resume["clientSessionId"].as_str().unwrap();
            let peer = "11111111-1111-4111-8111-111111111111";
            ws.send(Message::Text(json!({"protocolVersion":2,"type":"ready","clientSessionId":id,
                "serverSessionId":peer,"contiguousSeq":-1,"durability":"local-fsync-pg-commit",
                "limits":{"frameBytes":1048576,"batchBytes":2097152,"batchFrames":100,"flushMs":250}}).to_string().into())).await.unwrap();
            let batch: Value =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(
                STANDARD
                    .decode(batch["frames"][0]["data"].as_str().unwrap())
                    .unwrap(),
                b"saved offline"
            );
            ws.send(Message::Text(
                json!({"protocolVersion":2,"type":"ack","clientSessionId":id,
                "serverSessionId":peer,"contiguousSeq":0})
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        });
        // A different install must not upload the old scope even with consent.
        recorder
            .start(
                anywhere_on_loopback(),
                Some(url.clone()),
                Some("different-install-token".into()),
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!server.is_finished());
        assert_eq!(recorder.status().upstream.sent, 0);
        recorder.stop();
        recorder
            .start(
                anywhere_on_loopback(),
                Some(url),
                Some("test-install-token".into()),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("Saved backlog never resumed")
            .unwrap();
        assert!(within(5, || recorder.status().upstream.confirmed == 1).await);
        recorder.stop();
    }

    #[tokio::test]
    async fn turning_it_on_takes_the_socket_and_turning_it_off_gives_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new(dir.path().to_path_buf());
        let port = recorder
            .start(anywhere_on_loopback(), None, None)
            .await
            .unwrap();

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

        let first = recorder
            .start(anywhere_on_loopback(), None, None)
            .await
            .unwrap();
        let second = recorder
            .start(anywhere_on_loopback(), None, None)
            .await
            .unwrap();

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

            use base64::{engine::general_purpose::STANDARD, Engine};
            use serde_json::{json, Value};
            let resume: Value =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            let id = resume["clientSessionId"].as_str().unwrap();
            let peer = "11111111-1111-4111-8111-111111111111";
            ws.send(Message::Text(json!({"protocolVersion":2,"type":"ready","clientSessionId":id,
                "serverSessionId":peer,"contiguousSeq":-1,"durability":"local-fsync-pg-commit",
                "limits":{"frameBytes":1048576,"batchBytes":2097152,"batchFrames":100,"flushMs":250}}).to_string().into())).await.unwrap();
            while let Some(Ok(Message::Text(text))) = ws.next().await {
                let batch: Value = serde_json::from_str(&text).unwrap();
                for frame in batch["frames"].as_array().unwrap() {
                    let raw = STANDARD.decode(frame["data"].as_str().unwrap()).unwrap();
                    received
                        .lock()
                        .unwrap()
                        .push(String::from_utf8(raw).unwrap());
                }
                let seq = batch["frames"].as_array().unwrap().last().unwrap()["seq"]
                    .as_i64()
                    .unwrap();
                ws.send(Message::Text(
                    json!({"protocolVersion":2,"type":"ack","clientSessionId":id,
                    "serverSessionId":peer,"contiguousSeq":seq})
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new(dir.path().to_path_buf());
        let port = recorder
            .start(
                anywhere_on_loopback(),
                Some(url),
                Some("test-install-token".into()),
            )
            .await
            .unwrap();

        let mut game = game_connects(port).await;
        for frame in [r#"{"category":"init"}"#, r#"{"a":1}"#, r#"{"a":2}"#] {
            game.send(Message::Text(frame.into())).await.unwrap();
        }

        // A game disconnect closes its legacy export, while authorized
        // durable delivery remains owned by the recording switch.
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
