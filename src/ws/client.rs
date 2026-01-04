//! WebSocket client with automatic reconnection

use super::types::{WsConfig, WsError, WsMessage};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Reusable WebSocket client with automatic reconnection and ping/pong handling
pub struct WsClient {
    config: WsConfig,
}

impl WsClient {
    /// Create a new WebSocket client with the given configuration
    pub fn new(config: WsConfig) -> Self {
        Self { config }
    }

    /// Create a new client with just a URL using default config
    pub fn with_url(url: impl Into<String>) -> Self {
        Self::new(WsConfig::new(url))
    }

    /// Get the configured URL
    pub fn url(&self) -> &str {
        &self.config.url
    }

    /// Connect and return a receiver for messages
    ///
    /// This spawns a background task that handles connection management,
    /// automatic reconnection with exponential backoff, and ping/pong keepalive.
    ///
    /// Returns a channel receiver that will receive all WebSocket messages
    /// including connection status events (Connected, Disconnected, Reconnecting).
    pub fn connect(&self) -> mpsc::Receiver<WsMessage> {
        let (tx, rx) = mpsc::channel(1024);
        let config = self.config.clone();

        tokio::spawn(async move {
            if let Err(e) = Self::run_connection_loop(config, tx).await {
                tracing::error!(error = %e, "WebSocket connection loop failed");
            }
        });

        rx
    }

    /// Connect and return both a receiver and a sender for bidirectional communication
    ///
    /// The sender can be used to send messages to the WebSocket server.
    /// Returns (message_receiver, message_sender)
    pub fn connect_bidirectional(&self) -> (mpsc::Receiver<WsMessage>, mpsc::Sender<String>) {
        let (msg_tx, msg_rx) = mpsc::channel(1024);
        let (send_tx, send_rx) = mpsc::channel(256);
        let config = self.config.clone();

        tokio::spawn(async move {
            if let Err(e) = Self::run_bidirectional_loop(config, msg_tx, send_rx).await {
                tracing::error!(error = %e, "WebSocket bidirectional loop failed");
            }
        });

        (msg_rx, send_tx)
    }

    /// Run the connection loop with automatic reconnection
    async fn run_connection_loop(
        config: WsConfig,
        tx: mpsc::Sender<WsMessage>,
    ) -> Result<(), WsError> {
        let mut reconnect_attempts = 0;
        let mut reconnect_delay = config.initial_reconnect_delay;

        loop {
            match Self::connect_and_stream(&config, &tx, None).await {
                Ok(()) => {
                    tracing::info!("WebSocket connection closed cleanly");
                    let _ = tx.send(WsMessage::Disconnected).await;
                    break;
                }
                Err(e) => {
                    reconnect_attempts += 1;
                    tracing::warn!(
                        error = %e,
                        attempt = reconnect_attempts,
                        "WebSocket connection error, reconnecting..."
                    );

                    // Check max reconnects (0 = infinite)
                    if config.max_reconnect_attempts > 0
                        && reconnect_attempts >= config.max_reconnect_attempts
                    {
                        tracing::error!("Max reconnection attempts reached");
                        let _ = tx.send(WsMessage::Disconnected).await;
                        return Err(WsError::MaxReconnectsExceeded);
                    }

                    // Check if receiver is still alive
                    if tx.is_closed() {
                        tracing::info!("Receiver dropped, stopping reconnection");
                        break;
                    }

                    // Notify about reconnection attempt
                    let _ = tx
                        .send(WsMessage::Reconnecting {
                            attempt: reconnect_attempts,
                        })
                        .await;

                    sleep(reconnect_delay).await;
                    reconnect_delay = (reconnect_delay * 2).min(config.max_reconnect_delay);
                }
            }
        }

        Ok(())
    }

    /// Run bidirectional connection loop
    async fn run_bidirectional_loop(
        config: WsConfig,
        tx: mpsc::Sender<WsMessage>,
        send_rx: mpsc::Receiver<String>,
    ) -> Result<(), WsError> {
        let mut reconnect_attempts = 0;
        let mut reconnect_delay = config.initial_reconnect_delay;
        let mut send_rx = send_rx;

        loop {
            match Self::connect_and_stream(&config, &tx, Some(&mut send_rx)).await {
                Ok(()) => {
                    tracing::info!("WebSocket connection closed cleanly");
                    let _ = tx.send(WsMessage::Disconnected).await;
                    break;
                }
                Err(e) => {
                    reconnect_attempts += 1;
                    tracing::warn!(
                        error = %e,
                        attempt = reconnect_attempts,
                        "WebSocket connection error, reconnecting..."
                    );

                    if config.max_reconnect_attempts > 0
                        && reconnect_attempts >= config.max_reconnect_attempts
                    {
                        tracing::error!("Max reconnection attempts reached");
                        let _ = tx.send(WsMessage::Disconnected).await;
                        return Err(WsError::MaxReconnectsExceeded);
                    }

                    if tx.is_closed() {
                        tracing::info!("Receiver dropped, stopping reconnection");
                        break;
                    }

                    let _ = tx
                        .send(WsMessage::Reconnecting {
                            attempt: reconnect_attempts,
                        })
                        .await;

                    sleep(reconnect_delay).await;
                    reconnect_delay = (reconnect_delay * 2).min(config.max_reconnect_delay);
                }
            }
        }

        Ok(())
    }

    /// Connect to WebSocket and stream messages
    async fn connect_and_stream(
        config: &WsConfig,
        tx: &mpsc::Sender<WsMessage>,
        mut send_rx: Option<&mut mpsc::Receiver<String>>,
    ) -> Result<(), WsError> {
        tracing::info!(url = %config.url, "Connecting to WebSocket");

        let (ws_stream, _response) = connect_async(&config.url)
            .await
            .map_err(|e| WsError::ConnectionFailed(e.to_string()))?;

        let (mut write, mut read) = ws_stream.split();

        tracing::info!("WebSocket connected");

        // Notify connected
        if tx.send(WsMessage::Connected).await.is_err() {
            return Ok(());
        }

        // Setup ping interval
        let mut ping_interval = tokio::time::interval(config.ping_interval);
        ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Pong timeout tracking
        let mut waiting_for_pong = false;
        let pong_timeout = config.pong_timeout;

        loop {
            tokio::select! {
                // Handle incoming messages
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if tx.send(WsMessage::Text(text)).await.is_err() {
                                tracing::debug!("Receiver dropped, closing connection");
                                return Ok(());
                            }
                        }
                        Some(Ok(Message::Binary(data))) => {
                            if tx.send(WsMessage::Binary(data)).await.is_err() {
                                tracing::debug!("Receiver dropped, closing connection");
                                return Ok(());
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            write.send(Message::Pong(data)).await
                                .map_err(|e| WsError::SendFailed(e.to_string()))?;
                        }
                        Some(Ok(Message::Pong(_))) => {
                            waiting_for_pong = false;
                        }
                        Some(Ok(Message::Close(_))) => {
                            tracing::info!("Received close frame");
                            return Ok(());
                        }
                        Some(Err(e)) => {
                            return Err(WsError::ConnectionFailed(e.to_string()));
                        }
                        None => {
                            return Err(WsError::ConnectionFailed("Stream ended unexpectedly".into()));
                        }
                        _ => {}
                    }
                }

                // Handle outgoing messages if bidirectional
                msg = async {
                    match &mut send_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    match msg {
                        Some(text) => {
                            write.send(Message::Text(text)).await
                                .map_err(|e| WsError::SendFailed(e.to_string()))?;
                        }
                        None => {
                            // Sender dropped, close connection
                            return Ok(());
                        }
                    }
                }

                // Send periodic pings
                _ = ping_interval.tick() => {
                    if waiting_for_pong {
                        // Pong timeout, reconnect
                        return Err(WsError::ConnectionFailed("Pong timeout".into()));
                    }
                    write.send(Message::Ping(vec![])).await
                        .map_err(|e| WsError::SendFailed(e.to_string()))?;
                    waiting_for_pong = true;

                    // Schedule pong timeout check
                    tokio::spawn(async move {
                        tokio::time::sleep(pong_timeout).await;
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_ws_client_creation() {
        let client = WsClient::with_url("wss://example.com");
        assert_eq!(client.url(), "wss://example.com");
    }

    #[test]
    fn test_ws_client_with_config() {
        let config = WsConfig::new("wss://test.com")
            .max_reconnects(5)
            .ping_interval(Duration::from_secs(15));

        let client = WsClient::new(config);
        assert_eq!(client.url(), "wss://test.com");
        assert_eq!(client.config.max_reconnect_attempts, 5);
        assert_eq!(client.config.ping_interval, Duration::from_secs(15));
    }

    #[tokio::test]
    async fn test_ws_client_connection_failure() {
        // Connect to invalid URL should fail gracefully
        let client = WsClient::new(
            WsConfig::new("wss://invalid.localhost.test:12345")
                .max_reconnects(1)
                .initial_delay(Duration::from_millis(10)),
        );

        let mut rx = client.connect();

        // Should receive reconnecting and then disconnected
        let mut got_disconnect = false;
        let timeout = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(msg) = rx.recv().await {
                match msg {
                    WsMessage::Disconnected => {
                        got_disconnect = true;
                        break;
                    }
                    WsMessage::Reconnecting { .. } => continue,
                    _ => {}
                }
            }
        });

        timeout.await.expect("Test timed out");
        assert!(got_disconnect, "Should receive Disconnected message");
    }

    #[test]
    fn test_config_builder_chain() {
        let config = WsConfig::new("wss://example.com")
            .max_reconnects(3)
            .initial_delay(Duration::from_millis(100))
            .max_delay(Duration::from_secs(10))
            .ping_interval(Duration::from_secs(20));

        assert_eq!(config.max_reconnect_attempts, 3);
        assert_eq!(config.initial_reconnect_delay, Duration::from_millis(100));
        assert_eq!(config.max_reconnect_delay, Duration::from_secs(10));
        assert_eq!(config.ping_interval, Duration::from_secs(20));
    }
}
