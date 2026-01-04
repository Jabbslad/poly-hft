//! Binance WebSocket price feed implementation

use super::{PriceFeed, PriceTick};
use crate::ws::{WsClient, WsConfig, WsMessage};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::mpsc;

/// Binance WebSocket base URL
const BINANCE_WS_URL: &str = "wss://stream.binance.com:9443/ws";

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

    /// Run the message processing loop
    async fn run_message_loop(
        mut ws_rx: mpsc::Receiver<WsMessage>,
        tick_tx: mpsc::Sender<PriceTick>,
    ) {
        while let Some(msg) = ws_rx.recv().await {
            match msg {
                WsMessage::Text(text) => {
                    if let Some(tick) = Self::parse_message(&text) {
                        if tick_tx.send(tick).await.is_err() {
                            tracing::debug!("Tick receiver dropped, stopping feed");
                            break;
                        }
                    }
                }
                WsMessage::Connected => {
                    tracing::info!("Binance feed connected");
                }
                WsMessage::Disconnected => {
                    tracing::warn!("Binance feed disconnected");
                    break;
                }
                WsMessage::Reconnecting { attempt } => {
                    tracing::warn!(attempt, "Binance feed reconnecting...");
                }
                WsMessage::Binary(_) => {
                    // Binance doesn't send binary messages for trade streams
                }
            }
        }
    }
}

#[async_trait]
impl PriceFeed for BinanceFeed {
    async fn subscribe(&self) -> anyhow::Result<mpsc::Receiver<PriceTick>> {
        let (tick_tx, tick_rx) = mpsc::channel(1024);
        let url = self.build_ws_url();

        tracing::info!(symbol = %self.symbol, "Subscribing to Binance feed");

        // Create WebSocket client with config
        let config = WsConfig::new(url)
            .max_reconnects(10)
            .initial_delay(Duration::from_secs(1))
            .max_delay(Duration::from_secs(60))
            .ping_interval(Duration::from_secs(30));

        let client = WsClient::new(config);
        let ws_rx = client.connect();

        // Spawn message processing task
        tokio::spawn(async move {
            Self::run_message_loop(ws_rx, tick_tx).await;
        });

        Ok(tick_rx)
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

    #[tokio::test]
    async fn test_message_loop_handles_text() {
        let (ws_tx, ws_rx) = mpsc::channel(10);
        let (tick_tx, mut tick_rx) = mpsc::channel(10);

        // Spawn the message loop
        let handle = tokio::spawn(async move {
            BinanceFeed::run_message_loop(ws_rx, tick_tx).await;
        });

        // Send a valid trade message
        let msg = r#"{"e":"trade","E":1704067200000,"s":"BTCUSDT","t":123456789,"p":"42500.50","q":"0.001","T":1704067200123}"#;
        ws_tx.send(WsMessage::Text(msg.to_string())).await.unwrap();

        // Receive the tick
        let tick = tick_rx.recv().await.unwrap();
        assert_eq!(tick.symbol, "BTCUSDT");
        assert_eq!(tick.price, Decimal::from_str("42500.50").unwrap());

        // Send disconnect to close the loop
        ws_tx.send(WsMessage::Disconnected).await.unwrap();
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_message_loop_ignores_invalid() {
        let (ws_tx, ws_rx) = mpsc::channel(10);
        let (tick_tx, mut tick_rx) = mpsc::channel(10);

        let handle = tokio::spawn(async move {
            BinanceFeed::run_message_loop(ws_rx, tick_tx).await;
        });

        // Send invalid message
        ws_tx
            .send(WsMessage::Text("invalid json".to_string()))
            .await
            .unwrap();

        // Send valid message
        let msg = r#"{"e":"trade","E":1704067200000,"s":"BTCUSDT","t":123456789,"p":"100.00","q":"0.001","T":1704067200123}"#;
        ws_tx.send(WsMessage::Text(msg.to_string())).await.unwrap();

        // Should only receive the valid tick
        let tick = tick_rx.recv().await.unwrap();
        assert_eq!(tick.price, Decimal::from_str("100.00").unwrap());

        ws_tx.send(WsMessage::Disconnected).await.unwrap();
        handle.await.unwrap();
    }
}
