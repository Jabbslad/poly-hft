//! Binance WebSocket price feed implementation

use super::{PriceFeed, PriceTick};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Binance WebSocket base URL
const BINANCE_WS_URL: &str = "wss://stream.binance.com:9443/ws";

/// Maximum reconnection attempts before giving up
const MAX_RECONNECT_ATTEMPTS: u32 = 10;

/// Initial reconnection delay
const INITIAL_RECONNECT_DELAY: Duration = Duration::from_secs(1);

/// Maximum reconnection delay
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(60);

/// Binance trade message structure
#[derive(Debug, Deserialize)]
struct BinanceTradeMessage {
    /// Event type
    #[serde(rename = "e")]
    event_type: String,
    /// Event time (milliseconds)
    #[serde(rename = "E")]
    #[allow(dead_code)]
    event_time: i64,
    /// Symbol
    #[serde(rename = "s")]
    symbol: String,
    /// Trade ID
    #[serde(rename = "t")]
    #[allow(dead_code)]
    trade_id: u64,
    /// Price
    #[serde(rename = "p")]
    price: String,
    /// Quantity
    #[serde(rename = "q")]
    #[allow(dead_code)]
    quantity: String,
    /// Trade time (milliseconds)
    #[serde(rename = "T")]
    trade_time: i64,
}

/// Binance WebSocket feed for btcusdt@trade stream
pub struct BinanceFeed {
    symbol: String,
}

impl BinanceFeed {
    /// Create a new Binance feed for the given symbol
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into().to_lowercase(),
        }
    }

    /// Build the WebSocket URL for the trade stream
    fn build_ws_url(&self) -> String {
        format!("{}/{}@trade", BINANCE_WS_URL, self.symbol)
    }

    /// Parse a Binance trade message into a PriceTick
    fn parse_message(msg: &str) -> Option<PriceTick> {
        let trade: BinanceTradeMessage = serde_json::from_str(msg).ok()?;

        if trade.event_type != "trade" {
            return None;
        }

        let price = Decimal::from_str(&trade.price).ok()?;
        let exchange_ts = Utc.timestamp_millis_opt(trade.trade_time).single()?;
        let timestamp = Utc::now();

        Some(PriceTick {
            symbol: trade.symbol,
            price,
            timestamp,
            exchange_ts,
        })
    }

    /// Run the WebSocket connection loop with automatic reconnection
    async fn run_connection_loop(url: String, tx: mpsc::Sender<PriceTick>) -> anyhow::Result<()> {
        let mut reconnect_attempts = 0;
        let mut reconnect_delay = INITIAL_RECONNECT_DELAY;

        loop {
            match Self::connect_and_stream(&url, &tx).await {
                Ok(()) => {
                    // Clean disconnect, exit
                    tracing::info!("WebSocket connection closed cleanly");
                    break;
                }
                Err(e) => {
                    reconnect_attempts += 1;
                    tracing::warn!(
                        error = %e,
                        attempt = reconnect_attempts,
                        "WebSocket connection error, reconnecting..."
                    );

                    if reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
                        tracing::error!("Max reconnection attempts reached, giving up");
                        return Err(anyhow::anyhow!(
                            "Max reconnection attempts ({}) reached",
                            MAX_RECONNECT_ATTEMPTS
                        ));
                    }

                    // Check if receiver is still alive
                    if tx.is_closed() {
                        tracing::info!("Receiver dropped, stopping reconnection");
                        break;
                    }

                    sleep(reconnect_delay).await;
                    reconnect_delay = (reconnect_delay * 2).min(MAX_RECONNECT_DELAY);
                }
            }
        }

        Ok(())
    }

    /// Connect to WebSocket and stream messages
    async fn connect_and_stream(url: &str, tx: &mpsc::Sender<PriceTick>) -> anyhow::Result<()> {
        tracing::info!(url = url, "Connecting to Binance WebSocket");

        let (ws_stream, _response) = connect_async(url).await?;
        let (mut write, mut read) = ws_stream.split();

        tracing::info!("Connected to Binance WebSocket");

        // Ping interval for keepalive
        let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
        ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                // Handle incoming messages
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Some(tick) = Self::parse_message(&text) {
                                if tx.send(tick).await.is_err() {
                                    tracing::debug!("Receiver dropped, closing connection");
                                    return Ok(());
                                }
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            write.send(Message::Pong(data)).await?;
                        }
                        Some(Ok(Message::Close(_))) => {
                            tracing::info!("Received close frame");
                            return Ok(());
                        }
                        Some(Err(e)) => {
                            return Err(anyhow::anyhow!("WebSocket error: {}", e));
                        }
                        None => {
                            return Err(anyhow::anyhow!("WebSocket stream ended unexpectedly"));
                        }
                        _ => {}
                    }
                }

                // Send periodic pings
                _ = ping_interval.tick() => {
                    write.send(Message::Ping(vec![])).await?;
                }
            }
        }
    }
}

#[async_trait]
impl PriceFeed for BinanceFeed {
    async fn subscribe(&self) -> anyhow::Result<mpsc::Receiver<PriceTick>> {
        let (tx, rx) = mpsc::channel(1024);
        let url = self.build_ws_url();

        tracing::info!(symbol = %self.symbol, "Subscribing to Binance feed");

        // Spawn connection task
        tokio::spawn(async move {
            if let Err(e) = Self::run_connection_loop(url, tx).await {
                tracing::error!(error = %e, "Binance feed connection loop failed");
            }
        });

        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binance_feed_creation() {
        let feed = BinanceFeed::new("btcusdt");
        assert_eq!(feed.symbol, "btcusdt");
    }

    #[test]
    fn test_binance_feed_uppercase_symbol() {
        let feed = BinanceFeed::new("BTCUSDT");
        assert_eq!(feed.symbol, "btcusdt");
    }

    #[test]
    fn test_build_ws_url() {
        let feed = BinanceFeed::new("btcusdt");
        let url = feed.build_ws_url();
        assert_eq!(url, "wss://stream.binance.com:9443/ws/btcusdt@trade");
    }

    #[test]
    fn test_parse_valid_trade_message() {
        let msg = r#"{
            "e": "trade",
            "E": 1704067200000,
            "s": "BTCUSDT",
            "t": 123456789,
            "p": "42500.50",
            "q": "0.001",
            "T": 1704067200123
        }"#;

        let tick = BinanceFeed::parse_message(msg).unwrap();
        assert_eq!(tick.symbol, "BTCUSDT");
        assert_eq!(tick.price, Decimal::from_str("42500.50").unwrap());
    }

    #[test]
    fn test_parse_invalid_event_type() {
        let msg = r#"{
            "e": "aggTrade",
            "E": 1704067200000,
            "s": "BTCUSDT",
            "t": 123456789,
            "p": "42500.50",
            "q": "0.001",
            "T": 1704067200123
        }"#;

        assert!(BinanceFeed::parse_message(msg).is_none());
    }

    #[test]
    fn test_parse_invalid_json() {
        let msg = "not valid json";
        assert!(BinanceFeed::parse_message(msg).is_none());
    }

    #[test]
    fn test_parse_invalid_price() {
        let msg = r#"{
            "e": "trade",
            "E": 1704067200000,
            "s": "BTCUSDT",
            "t": 123456789,
            "p": "not_a_number",
            "q": "0.001",
            "T": 1704067200123
        }"#;

        assert!(BinanceFeed::parse_message(msg).is_none());
    }
}
