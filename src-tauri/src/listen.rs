//! The socket Apex dials. The game connects out, so this end holds the
//! listening socket and hands whatever arrives to a callback verbatim - a
//! frame is bytes here, never an event.

use futures_util::StreamExt;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::{protocol::WebSocketConfig, Error, Message};

pub enum Frame {
    Text(String),
    Binary(Vec<u8>),
    Closed,
    Failure(&'static str),
}

pub async fn bind(addr: SocketAddr) -> std::io::Result<TcpListener> {
    TcpListener::bind(addr).await
}

/// One connection at a time, on purpose: Apex opens exactly one, and serving
/// several would mean two writers on one session log.
pub async fn serve(
    listener: TcpListener,
    mut on_frame: impl FnMut(Frame) + Send + 'static,
) -> std::io::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(8 * 1024 * 1024))
            .max_frame_size(Some(8 * 1024 * 1024));
        let Ok(mut ws) = tokio_tungstenite::accept_async_with_config(stream, Some(config)).await
        else {
            continue;
        };

        while let Some(message) = ws.next().await {
            let message = match message {
                Ok(message) => message,
                Err(Error::Capacity(_)) => {
                    on_frame(Frame::Failure("capture-frame-too-large"));
                    break;
                }
                Err(_) => break,
            };
            match message {
                Message::Text(text) => on_frame(Frame::Text(text.to_string())),
                Message::Binary(bytes) => on_frame(Frame::Binary(bytes.to_vec())),
                Message::Close(_) => break,
                _ => {}
            }
        }

        on_frame(Frame::Closed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::SinkExt;
    use std::sync::{Arc, Mutex};
    use tokio_tungstenite::connect_async;

    #[tokio::test]
    async fn oversized_capture_is_reported_as_a_failure_instead_of_a_normal_disconnect() {
        let failed = Arc::new(Mutex::new(false));
        let sink = failed.clone();
        let listener = bind("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            serve(listener, move |frame| {
                if let Frame::Failure(code) = frame {
                    assert_eq!(code, "capture-frame-too-large");
                    *sink.lock().unwrap() = true;
                }
            })
            .await
            .unwrap();
        });
        let (mut game, _) = connect_async(format!("ws://127.0.0.1:{port}/"))
            .await
            .unwrap();
        let _ = game
            .send(Message::Binary(vec![0; 9 * 1024 * 1024].into()))
            .await;
        for _ in 0..250 {
            if *failed.lock().unwrap() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(
            *failed.lock().unwrap(),
            "Oversized capture vanished without a failure state"
        );
        server.abort();
    }

    #[tokio::test]
    async fn text_frames_arrive_in_order() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let bound = bind(addr).await.unwrap();
        let port = bound.local_addr().unwrap().port();

        tokio::spawn(async move {
            serve(bound, move |frame| {
                if let Frame::Text(text) = frame {
                    sink.lock().unwrap().push(text);
                }
            })
            .await
            .unwrap();
        });

        let (mut client, _) = connect_async(format!("ws://127.0.0.1:{port}/"))
            .await
            .unwrap();
        client.send(Message::Text("one".into())).await.unwrap();
        client.send(Message::Text("two".into())).await.unwrap();
        client.close(None).await.unwrap();

        for _ in 0..50 {
            if seen.lock().unwrap().len() == 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(*seen.lock().unwrap(), vec!["one", "two"]);
    }
}
