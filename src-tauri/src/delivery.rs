//! Acknowledged delivery reads a bounded window from retained local evidence.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::{HeaderValue, AUTHORIZATION};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{Error, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::journal::{Journal, Kind, MAX_BATCH_BYTES, MAX_BATCH_FRAMES, MAX_FRAME_BYTES};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryState {
    Idle,
    Connecting,
    Durable,
    Legacy,
    Retrying,
    Refused,
    Conflict,
    LocalFailure,
    Incompatible,
}

struct Shared {
    journal: Arc<Journal>,
    confirmed_seq: Mutex<i64>,
    sent: AtomicU64,
    state: Mutex<DeliveryState>,
    retry_seconds: AtomicU64,
    wake: Notify,
    stopped: AtomicBool,
}

/// Own the task for exactly as long as upload consent remains active.
pub struct DurableUpstream {
    shared: Arc<Shared>,
    task: JoinHandle<()>,
}

impl DurableUpstream {
    pub fn start(url: String, token: String, journal: Arc<Journal>) -> Self {
        let shared = Arc::new(Shared {
            journal,
            confirmed_seq: Mutex::new(-1),
            sent: AtomicU64::new(0),
            state: Mutex::new(DeliveryState::Idle),
            retry_seconds: AtomicU64::new(0),
            wake: Notify::new(),
            stopped: AtomicBool::new(false),
        });
        let task = tokio::spawn(run(url, token, shared.clone()));
        Self { shared, task }
    }

    pub fn wake(&self) {
        self.shared.wake.notify_one();
    }
    pub fn confirmed(&self) -> u64 {
        (*self.shared.confirmed_seq.lock().unwrap() + 1) as u64
    }
    pub fn sent(&self) -> u64 {
        self.shared.sent.load(Ordering::Acquire)
    }
    pub fn pending(&self) -> u64 {
        (self.shared.journal.last_seq() - *self.shared.confirmed_seq.lock().unwrap()).max(0) as u64
    }
    pub fn state(&self) -> DeliveryState {
        *self.shared.state.lock().unwrap()
    }
    pub fn retry_seconds(&self) -> u64 {
        self.shared.retry_seconds.load(Ordering::Acquire)
    }
}

impl Drop for DurableUpstream {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Release);
        self.shared.wake.notify_waiters();
        self.task.abort();
    }
}

enum Fault {
    Retry,
    Refused,
    Conflict,
    Local,
    Incompatible,
    UseLegacy,
}

impl Shared {
    fn state(&self, state: DeliveryState) {
        *self.state.lock().unwrap() = state;
    }
    fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
    fn cursor(&self) -> i64 {
        *self.confirmed_seq.lock().unwrap()
    }
    fn confirm(&self, seq: i64) {
        *self.confirmed_seq.lock().unwrap() = seq;
    }
}

async fn connect(url: &str, token: &str, legacy: bool) -> Result<Socket, Fault> {
    let mut url = reqwest::Url::parse(url).map_err(|_| Fault::Incompatible)?;
    if !matches!(url.scheme(), "ws" | "wss")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Fault::Incompatible);
    }
    url.set_path(if legacy { "/" } else { "/v2" });
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|_| Fault::Incompatible)?;
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| Fault::Refused)?,
    );
    let config = WebSocketConfig::default()
        .max_message_size(Some(8192))
        .max_frame_size(Some(8192))
        .write_buffer_size(65536)
        .max_write_buffer_size(4 * 1024 * 1024);
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::connect_async_with_config(request, Some(config), true),
    )
    .await;
    match result {
        Ok(Ok((ws, _))) => Ok(ws),
        Ok(Err(Error::Http(response))) => match response.status().as_u16() {
            401 | 403 => Err(Fault::Refused),
            404 | 426 if !legacy => Err(Fault::UseLegacy),
            _ => Err(Fault::Retry),
        },
        _ => Err(Fault::Retry),
    }
}

async fn value(ws: &mut Socket) -> Result<Value, Fault> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(raw))) => {
                    return serde_json::from_str(&raw).map_err(|_| Fault::Incompatible)
                }
                Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {
                    ws.flush().await.map_err(|_| Fault::Retry)?;
                }
                _ => return Err(Fault::Retry),
            }
        }
    })
    .await
    .map_err(|_| Fault::Retry)?
}

fn acknowledgement(
    value: &Value,
    kind: &str,
    journal: &Journal,
    peer: Option<&str>,
    minimum: i64,
) -> Result<(i64, String), Fault> {
    if value["type"] == "error" && value["code"] == "conflicting-sequence" {
        return Err(Fault::Conflict);
    }
    if value["protocolVersion"].as_u64() != Some(2)
        || value["type"] != kind
        || value["clientSessionId"].as_str() != Some(journal.session_id())
    {
        return Err(Fault::Incompatible);
    }
    let seq = value["contiguousSeq"].as_i64().ok_or(Fault::Incompatible)?;
    let id = value["serverSessionId"]
        .as_str()
        .ok_or(Fault::Incompatible)?;
    if peer.is_some_and(|expected| expected != id) || seq < minimum || seq > journal.last_seq() {
        return Err(Fault::Conflict);
    }
    if kind == "ready" {
        if value["durability"] != "local-fsync-pg-commit"
            || value["limits"]["frameBytes"].as_u64().unwrap_or(0) < MAX_FRAME_BYTES as u64
            || value["limits"]["batchBytes"].as_u64().unwrap_or(0) < MAX_BATCH_BYTES as u64
            || value["limits"]["batchFrames"].as_u64().unwrap_or(0) < MAX_BATCH_FRAMES as u64
            || !matches!(value["limits"]["flushMs"].as_u64(), Some(1..=250))
        {
            return Err(Fault::Incompatible);
        }
        journal.bind_server(id).map_err(|error| {
            if error.kind() == std::io::ErrorKind::InvalidData {
                Fault::Conflict
            } else {
                Fault::Local
            }
        })?;
    }
    Ok((seq, id.to_string()))
}

async fn durable(ws: &mut Socket, shared: &Shared) -> Result<(), Fault> {
    let journal = &shared.journal;
    ws.send(Message::Text(
        json!({"protocolVersion":2,"type":"resume","clientSessionId":journal.session_id()})
            .to_string()
            .into(),
    ))
    .await
    .map_err(|_| Fault::Retry)?;
    let reply = value(ws).await?;
    let (seq, peer) = acknowledgement(&reply, "ready", journal, None, -1)?;
    let flush_ms = reply["limits"]["flushMs"].as_u64().unwrap();
    shared.confirm(seq);
    shared.state(DeliveryState::Durable);
    let mut reader = journal.reader(seq + 1).map_err(|_| Fault::Local)?;
    loop {
        if shared.stopped() {
            return Ok(());
        }
        if shared.cursor() >= journal.last_seq() {
            tokio::select! {
                _ = shared.wake.notified() => {},
                incoming = ws.next() => match incoming {
                    Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => { ws.flush().await.map_err(|_| Fault::Retry)?; },
                    _ => return Err(Fault::Retry),
                },
            }
            continue;
        }
        // One deadline, never reset by newly arriving frames.
        tokio::time::sleep(Duration::from_millis(flush_ms)).await;
        let batch = reader.batch().map_err(|_| Fault::Local)?;
        if batch.is_empty() {
            continue;
        }
        let last = batch.last().unwrap().seq;
        let payload = json!({"protocolVersion":2,"clientSessionId":journal.session_id(),
            "frames":batch.iter().map(|frame| frame.wire()).collect::<Vec<_>>()});
        if shared.stopped() {
            return Ok(());
        }
        ws.send(Message::Text(payload.to_string().into()))
            .await
            .map_err(|_| Fault::Retry)?;
        shared.sent.fetch_add(batch.len() as u64, Ordering::AcqRel);
        let (seq, _) = acknowledgement(&value(ws).await?, "ack", journal, Some(&peer), last)?;
        shared.confirm(seq);
        if seq != last {
            reader = journal.reader(seq + 1).map_err(|_| Fault::Local)?;
        }
    }
}

async fn legacy(ws: &mut Socket, shared: &Shared, cursor: &mut i64) -> Result<(), Fault> {
    shared.state(DeliveryState::Legacy);
    let mut reader = shared
        .journal
        .reader(*cursor + 1)
        .map_err(|_| Fault::Local)?;
    loop {
        if shared.stopped() {
            return Ok(());
        }
        let frames = reader.batch().map_err(|_| Fault::Local)?;
        if frames.is_empty() {
            tokio::select! {
                _ = shared.wake.notified() => {},
                incoming = ws.next() => match incoming {
                    Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => { ws.flush().await.map_err(|_| Fault::Retry)?; },
                    _ => return Err(Fault::Retry),
                },
            }
            continue;
        }
        for frame in frames {
            if shared.stopped() {
                return Ok(());
            }
            let message = match frame.kind {
                Kind::Text => Message::Text(
                    String::from_utf8(frame.data)
                        .map_err(|_| Fault::Local)?
                        .into(),
                ),
                Kind::Binary => Message::Binary(frame.data.into()),
            };
            ws.send(message).await.map_err(|_| Fault::Retry)?;
            *cursor = frame.seq;
            shared.sent.fetch_add(1, Ordering::AcqRel);
        }
    }
}

async fn run(url: String, token: String, shared: Arc<Shared>) {
    let mut failures = 0u32;
    let mut use_legacy = false;
    let mut legacy_cursor = -1;
    while !shared.stopped() {
        if shared.journal.last_seq() < 0 {
            shared.state(DeliveryState::Idle);
            shared.wake.notified().await;
            continue;
        }
        shared.state(DeliveryState::Connecting);
        let outcome = match connect(&url, &token, use_legacy).await {
            Ok(mut ws) => {
                failures = 0;
                if use_legacy {
                    legacy(&mut ws, &shared, &mut legacy_cursor).await
                } else {
                    durable(&mut ws, &shared).await
                }
            }
            Err(error) => Err(error),
        };
        match outcome {
            Ok(()) => return,
            Err(Fault::UseLegacy) => {
                use_legacy = true;
                continue;
            }
            Err(Fault::Refused) => {
                shared.state(DeliveryState::Refused);
                return;
            }
            Err(Fault::Conflict) => {
                shared.state(DeliveryState::Conflict);
                return;
            }
            Err(Fault::Local) => {
                shared.state(DeliveryState::LocalFailure);
                return;
            }
            Err(Fault::Incompatible) => {
                shared.state(DeliveryState::Incompatible);
                return;
            }
            Err(Fault::Retry) => {}
        }
        let seconds = crate::upstream::backoff_seconds(failures);
        failures = failures.saturating_add(1);
        shared.retry_seconds.store(seconds, Ordering::Release);
        shared.state(DeliveryState::Retrying);
        tokio::time::sleep(Duration::from_secs(seconds)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{Budget, Journal, Kind};
    use futures_util::{SinkExt, StreamExt};
    use serde_json::{json, Value};
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio::sync::Notify;
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
    use tokio_tungstenite::tungstenite::http::StatusCode;
    use tokio_tungstenite::tungstenite::Message;

    const PEER: &str = "11111111-1111-4111-8111-111111111111";

    fn ready(id: &str, seq: i64) -> Value {
        json!({"protocolVersion":2,"type":"ready","clientSessionId":id,"serverSessionId":PEER,
            "contiguousSeq":seq,"durability":"local-fsync-pg-commit",
            "limits":{"frameBytes":1048576,"batchBytes":2097152,"batchFrames":100,"flushMs":250}})
    }

    fn ack(id: &str, seq: i64) -> Value {
        json!({"protocolVersion":2,"type":"ack","clientSessionId":id,"serverSessionId":PEER,"contiguousSeq":seq})
    }

    async fn within(mut predicate: impl FnMut() -> bool) {
        for _ in 0..1000 {
            if predicate() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(predicate(), "Delivery condition did not settle");
    }

    #[tokio::test]
    async fn lost_ack_retries_exact_evidence_and_socket_send_never_counts_as_confirmed() {
        let dir = tempfile::tempdir().unwrap();
        let budget = Budget::open(dir.path(), 8 * 1024 * 1024).unwrap();
        let journal =
            Arc::new(Journal::create(dir.path(), Some("test-install-token"), budget).unwrap());
        journal.append(Kind::Text, b"{ \"future\": 1 }").unwrap();
        journal.append(Kind::Binary, &[0, 255]).unwrap();
        let original = std::fs::read(journal.path()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", listener.local_addr().unwrap());
        let received = Arc::new(Mutex::new(Vec::<Value>::new()));
        let seen = received.clone();
        let committed = Arc::new(Notify::new());
        let committed_server = committed.clone();
        let release = Arc::new(Notify::new());
        let release_server = release.clone();
        let server = tokio::spawn(async move {
            let mut ledger = BTreeMap::new();
            let mut session = None;
            for attempt in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_hdr_async(
                    stream,
                    |r: &Request, response: Response| {
                        assert_eq!(r.uri().path(), "/v2");
                        assert_eq!(r.headers()["Authorization"], "Bearer test-install-token");
                        Ok(response)
                    },
                )
                .await
                .unwrap();
                let resume: Value =
                    serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap())
                        .unwrap();
                let id = resume["clientSessionId"].as_str().unwrap().to_string();
                if let Some(previous) = &session {
                    assert_eq!(previous, &id);
                }
                session = Some(id.clone());
                // Deliberately report an older cursor to exercise an idempotent replay too.
                ws.send(Message::Text(ready(&id, -1).to_string().into()))
                    .await
                    .unwrap();
                let batch: Value =
                    serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap())
                        .unwrap();
                for frame in batch["frames"].as_array().unwrap() {
                    let seq = frame["seq"].as_i64().unwrap();
                    if let Some(old) = ledger.insert(seq, frame.clone()) {
                        assert_eq!(old, *frame);
                    }
                    seen.lock().unwrap().push(frame.clone());
                }
                if attempt == 0 {
                    committed_server.notify_one();
                    release_server.notified().await;
                    ws.close(None).await.unwrap();
                } else {
                    assert_eq!(ledger.len(), 2);
                    ws.send(Message::Text(ack(&id, 1).to_string().into()))
                        .await
                        .unwrap();
                    while ws.next().await.is_some() {}
                }
            }
        });
        let sender = DurableUpstream::start(url, "test-install-token".into(), journal.clone());
        tokio::time::timeout(Duration::from_secs(5), committed.notified())
            .await
            .unwrap();
        assert_eq!(sender.confirmed(), 0);
        assert_eq!(sender.pending(), 2);
        release.notify_one();
        within(|| sender.confirmed() == 2).await;
        assert_eq!(sender.pending(), 0);
        assert_eq!(sender.state(), DeliveryState::Durable);
        drop(sender);
        server.await.unwrap();
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 4);
        assert_eq!(&frames[..2], &frames[2..]);
        assert_eq!(std::fs::read(journal.path()).unwrap(), original);
    }

    #[tokio::test]
    async fn auth_refusal_never_downgrades_or_discards_the_local_journal() {
        let dir = tempfile::tempdir().unwrap();
        let budget = Budget::open(dir.path(), 1024 * 1024).unwrap();
        let journal =
            Arc::new(Journal::create(dir.path(), Some("test-install-token"), budget).unwrap());
        journal.append(Kind::Text, b"private").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let result = tokio_tungstenite::accept_hdr_async(stream, |r: &Request, _: Response| {
                assert_eq!(r.uri().path(), "/v2");
                let mut refused = ErrorResponse::new(None);
                *refused.status_mut() = StatusCode::UNAUTHORIZED;
                Err(refused)
            })
            .await;
            assert!(result.is_err());
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let sender = DurableUpstream::start(url, "test-install-token".into(), journal.clone());
        within(|| sender.state() == DeliveryState::Refused).await;
        assert_eq!(sender.confirmed(), 0);
        assert_eq!(sender.pending(), 1);
        assert!(journal.path().exists());
        drop(sender);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn a_new_process_resumes_the_same_journal_from_the_server_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let budget = Budget::open(dir.path(), 1024 * 1024).unwrap();
        let journal = Arc::new(
            Journal::create(dir.path(), Some("test-install-token"), budget.clone()).unwrap(),
        );
        journal.append(Kind::Text, b"exact old bytes").unwrap();
        let path = journal.path().to_path_buf();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", listener.local_addr().unwrap());
        let id = journal.session_id().to_string();
        let server = tokio::spawn(async move {
            for attempt in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let resume: Value =
                    serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap())
                        .unwrap();
                assert_eq!(resume["clientSessionId"], id);
                ws.send(Message::Text(
                    ready(&id, if attempt == 0 { -1 } else { 0 })
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
                if attempt == 0 {
                    let batch: Value =
                        serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap())
                            .unwrap();
                    assert_eq!(batch["frames"][0]["seq"], 0);
                    ws.send(Message::Text(ack(&id, 0).to_string().into()))
                        .await
                        .unwrap();
                }
                while let Some(Ok(message)) = ws.next().await {
                    assert!(
                        !message.is_text(),
                        "Already confirmed frames were sent after process restart"
                    );
                }
            }
        });
        let sender = DurableUpstream::start(url.clone(), "test-install-token".into(), journal);
        within(|| sender.confirmed() == 1).await;
        drop(sender);
        let recovered = Arc::new(Journal::open(&path, Some("test-install-token"), budget).unwrap());
        let sender = DurableUpstream::start(url, "test-install-token".into(), recovered);
        within(|| sender.confirmed() == 1).await;
        assert_eq!(sender.sent(), 0);
        drop(sender);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn dropping_upload_consent_stops_work_and_keeps_pending_frames() {
        let dir = tempfile::tempdir().unwrap();
        let budget = Budget::open(dir.path(), 1024 * 1024).unwrap();
        let journal =
            Arc::new(Journal::create(dir.path(), Some("test-install-token"), budget).unwrap());
        journal.append(Kind::Text, b"pending").unwrap();
        let original = std::fs::read(journal.path()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", listener.local_addr().unwrap());
        let received = Arc::new(Notify::new());
        let seen = received.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let resume: Value =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            let id = resume["clientSessionId"].as_str().unwrap();
            ws.send(Message::Text(ready(id, -1).to_string().into()))
                .await
                .unwrap();
            let _ = ws.next().await.unwrap().unwrap();
            seen.notify_one();
            assert!(tokio::time::timeout(Duration::from_secs(2), ws.next())
                .await
                .is_ok());
        });
        let sender = DurableUpstream::start(url, "test-install-token".into(), journal.clone());
        tokio::time::timeout(Duration::from_secs(5), received.notified())
            .await
            .unwrap();
        assert_eq!(sender.pending(), 1);
        assert_eq!(sender.confirmed(), 0);
        drop(sender);
        server.await.unwrap();
        assert_eq!(std::fs::read(journal.path()).unwrap(), original);
    }

    #[tokio::test]
    async fn explicit_v1_fallback_reports_sent_without_claiming_durable_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let budget = Budget::open(dir.path(), 1024 * 1024).unwrap();
        let journal =
            Arc::new(Journal::create(dir.path(), Some("test-install-token"), budget).unwrap());
        journal.append(Kind::Text, b"{ \"future\": 2 }").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let result = tokio_tungstenite::accept_hdr_async(stream, |r: &Request, _: Response| {
                assert_eq!(r.uri().path(), "/v2");
                let mut refused = ErrorResponse::new(None);
                *refused.status_mut() = StatusCode::NOT_FOUND;
                Err(refused)
            })
            .await;
            assert!(result.is_err());
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws =
                tokio_tungstenite::accept_hdr_async(stream, |r: &Request, response: Response| {
                    assert_eq!(r.uri().path(), "/");
                    Ok(response)
                })
                .await
                .unwrap();
            assert_eq!(
                ws.next().await.unwrap().unwrap().to_text().unwrap(),
                "{ \"future\": 2 }"
            );
            while ws.next().await.is_some() {}
        });
        let sender = DurableUpstream::start(url, "test-install-token".into(), journal.clone());
        within(|| sender.sent() == 1).await;
        assert_eq!(sender.state(), DeliveryState::Legacy);
        assert_eq!(sender.confirmed(), 0);
        assert_eq!(sender.pending(), 1);
        drop(sender);
        server.await.unwrap();
        assert!(journal.path().exists());
    }
}
