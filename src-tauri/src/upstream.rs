//! Retry timing only. Pending frame storage belongs to the disk journal.

pub fn backoff_seconds(failures: u32) -> u64 {
    1u64.checked_shl(failures).unwrap_or(60).min(60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backoff_grows_and_is_capped() {
        assert_eq!(backoff_seconds(0), 1);
        assert_eq!(backoff_seconds(1), 2);
        assert_eq!(backoff_seconds(2), 4);
        assert_eq!(backoff_seconds(9), 60);
        assert_eq!(backoff_seconds(u32::MAX), 60);
    }
    #[tokio::test]
    async fn a_wss_url_reaches_the_network_rather_than_a_missing_feature() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("wss://{}/", listener.local_addr().unwrap());
        tokio::spawn(async move {
            drop(listener.accept().await);
        });
        let error = tokio_tungstenite::connect_async(url).await.unwrap_err();
        assert!(
            !matches!(error, tokio_tungstenite::tungstenite::Error::Url(_)),
            "TLS did not reach the network"
        );
    }
}
