//! Polymarket WebSocket client for order book updates
//!
//! Connects to the Polymarket CLOB WebSocket API to receive real-time
//! order book updates for specified token IDs.

use super::{OrderBook, PriceLevel};
use crate::ws::{WsClient, WsConfig, WsMessage};
use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::mpsc;

/// Polymarket CLOB WebSocket URL for market data
pub const POLYMARKET_WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";

/// Configuration for the Polymarket client
#[derive(Debug, Clone)]
pub struct PolymarketConfig {
    /// WebSocket URL (defaults to POLYMARKET_WS_URL)
    pub ws_url: String,
    /// Maximum reconnection attempts (0 = infinite)
    pub max_reconnects: u32,
    /// Initial reconnection delay
    pub initial_delay: Duration,
    /// Maximum reconnection delay
    pub max_delay: Duration,
    /// Channel buffer size for order book updates
    pub buffer_size: usize,
}

impl Default for PolymarketConfig {
    fn default() -> Self {
        Self {
            ws_url: POLYMARKET_WS_URL.to_string(),
            max_reconnects: 0, // Infinite retries
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            buffer_size: 256,
        }
    }
}

/// Polymarket WebSocket client for order book updates
pub struct PolymarketClient {
    config: PolymarketConfig,
}

impl PolymarketClient {
    /// Create a new Polymarket client with default configuration
    pub fn new() -> Self {
        Self {
            config: PolymarketConfig::default(),
        }
    }

    /// Create a new client with custom configuration
    pub fn with_config(config: PolymarketConfig) -> Self {
        Self { config }
    }

    /// Subscribe to order book updates for multiple tokens
    ///
    /// Returns a receiver that will emit OrderBook updates for all subscribed tokens.
    /// The client handles reconnection and resubscription automatically.
    pub async fn subscribe(
        &self,
        token_ids: Vec<String>,
    ) -> anyhow::Result<mpsc::Receiver<OrderBook>> {
        let (tx, rx) = mpsc::channel(self.config.buffer_size);

        if token_ids.is_empty() {
            tracing::warn!("No token IDs provided, returning empty receiver");
            return Ok(rx);
        }

        let config = self.config.clone();
        let tokens = token_ids.clone();

        tokio::spawn(async move {
            if let Err(e) = run_subscription_loop(config, tokens, tx).await {
                tracing::error!(error = %e, "Polymarket subscription loop failed");
            }
        });

        tracing::info!(
            token_count = token_ids.len(),
            "Started Polymarket order book subscription"
        );

        Ok(rx)
    }

    /// Subscribe to a single token's order book
    pub async fn subscribe_single(
        &self,
        token_id: &str,
    ) -> anyhow::Result<mpsc::Receiver<OrderBook>> {
        self.subscribe(vec![token_id.to_string()]).await
    }
}

impl Default for PolymarketClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Run the subscription loop with automatic reconnection
async fn run_subscription_loop(
    config: PolymarketConfig,
    token_ids: Vec<String>,
    tx: mpsc::Sender<OrderBook>,
) -> anyhow::Result<()> {
    let ws_config = WsConfig::new(&config.ws_url)
        .max_reconnects(config.max_reconnects)
        .initial_delay(config.initial_delay)
        .max_delay(config.max_delay);

    let ws_client = WsClient::new(ws_config);
    let (mut ws_rx, ws_tx) = ws_client.connect_bidirectional();

    // Track connection state for resubscription
    let mut connected = false;

    loop {
        tokio::select! {
            msg = ws_rx.recv() => {
                match msg {
                    Some(WsMessage::Connected) => {
                        tracing::info!("Polymarket WebSocket connected");
                        connected = true;

                        // Send subscription message
                        let sub_msg = SubscriptionMessage {
                            assets_ids: token_ids.clone(),
                            msg_type: "market".to_string(),
                        };

                        if let Ok(json) = serde_json::to_string(&sub_msg) {
                            if ws_tx.send(json).await.is_err() {
                                tracing::error!("Failed to send subscription message");
                                break;
                            }
                            tracing::info!(
                                tokens = token_ids.len(),
                                "Sent subscription for tokens"
                            );
                        }
                    }
                    Some(WsMessage::Text(text)) => {
                        if !connected {
                            continue;
                        }

                        // Log raw message for debugging
                        tracing::debug!(
                            msg_len = text.len(),
                            preview = %text.chars().take(200).collect::<String>(),
                            "Received Polymarket message"
                        );

                        // Parse the message and convert to OrderBook
                        match parse_market_message(&text) {
                            Ok(Some(order_book)) => {
                                if tx.send(order_book).await.is_err() {
                                    tracing::debug!("OrderBook receiver dropped");
                                    break;
                                }
                            }
                            Ok(None) => {
                                // Message parsed but not an order book update
                            }
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    msg_preview = %text.chars().take(100).collect::<String>(),
                                    "Failed to parse market message"
                                );
                            }
                        }
                    }
                    Some(WsMessage::Reconnecting { attempt }) => {
                        tracing::info!(attempt, "Polymarket WebSocket reconnecting");
                        connected = false;
                    }
                    Some(WsMessage::Disconnected) => {
                        tracing::info!("Polymarket WebSocket disconnected");
                        break;
                    }
                    Some(WsMessage::Binary(_)) => {
                        // Ignore binary messages
                    }
                    None => {
                        tracing::info!("Polymarket WebSocket channel closed");
                        break;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received shutdown signal");
                break;
            }
        }
    }

    Ok(())
}

/// Subscription message for Polymarket WebSocket
#[derive(Debug, Serialize)]
struct SubscriptionMessage {
    assets_ids: Vec<String>,
    #[serde(rename = "type")]
    msg_type: String,
}

/// Order book snapshot from Polymarket
#[derive(Debug, Deserialize)]
struct BookEvent {
    asset_id: String,
    #[serde(default)]
    bids: Vec<BookLevel>,
    #[serde(default)]
    asks: Vec<BookLevel>,
    #[serde(default)]
    #[allow(dead_code)]
    hash: String,
    #[serde(default)]
    timestamp: String,
    #[serde(default)]
    #[allow(dead_code)]
    market: String,
}

/// Price level in the order book
#[derive(Debug, Deserialize)]
struct BookLevel {
    price: String,
    size: String,
}

/// Price changes message from Polymarket WebSocket
/// Format: {"market": "0x...", "price_changes": [...]}
#[derive(Debug, Deserialize)]
struct PriceChangesMessage {
    #[allow(dead_code)]
    market: Option<String>,
    price_changes: Vec<PriceChange>,
}

/// Individual price change within a price_changes message
#[derive(Debug, Deserialize)]
struct PriceChange {
    /// Asset ID
    asset_id: String,
    /// Price level that changed
    price: String,
    /// New size at this price (0 means removed)
    size: String,
    /// Side: "BUY" or "SELL"
    side: String,
}

/// Parse a market message from the WebSocket
fn parse_market_message(text: &str) -> anyhow::Result<Option<OrderBook>> {
    // Try parsing as a list of events first (common format)
    if let Ok(events) = serde_json::from_str::<Vec<serde_json::Value>>(text) {
        tracing::trace!(event_count = events.len(), "Parsed as array");
        for event in events {
            match parse_single_event(&event) {
                Ok(Some(order_book)) => return Ok(Some(order_book)),
                Ok(None) => continue,
                Err(e) => {
                    tracing::debug!(error = %e, "Failed to parse event in array");
                    continue;
                }
            }
        }
        return Ok(None);
    }

    // Try parsing as a single event
    if let Ok(event) = serde_json::from_str::<serde_json::Value>(text) {
        return parse_single_event(&event);
    }

    Ok(None)
}

/// Parse a single event object
fn parse_single_event(event: &serde_json::Value) -> anyhow::Result<Option<OrderBook>> {
    // Check event type
    let event_type = event
        .get("event_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let has_bids = event.get("bids").is_some();
    let has_asks = event.get("asks").is_some();
    let has_asset_id = event.get("asset_id").is_some();

    tracing::trace!(
        event_type = %event_type,
        has_bids,
        has_asks,
        has_asset_id,
        "Parsing event"
    );

    match event_type {
        "book" => {
            // Full order book snapshot
            let book: BookEvent = serde_json::from_value(event.clone())?;
            let orderbook = book_event_to_orderbook(book)?;
            tracing::debug!(
                token_id = %orderbook.token_id,
                bid_count = orderbook.bids.len(),
                ask_count = orderbook.asks.len(),
                best_bid = ?orderbook.best_bid(),
                best_ask = ?orderbook.best_ask(),
                "Parsed order book"
            );
            Ok(Some(orderbook))
        }
        "price_change" => {
            // Parse price_changes array from the event
            if event.get("price_changes").is_some() {
                let msg: PriceChangesMessage = serde_json::from_value(event.clone())?;
                price_changes_to_orderbook(msg)
            } else {
                // Single price change without array - skip
                Ok(None)
            }
        }
        "last_trade_price" | "tick_size_change" => {
            // Informational events, don't generate order book
            Ok(None)
        }
        "" => {
            // Try to parse as a book event without event_type field
            if event.get("asset_id").is_some()
                && (event.get("bids").is_some() || event.get("asks").is_some())
            {
                let book: BookEvent = serde_json::from_value(event.clone())?;
                Ok(Some(book_event_to_orderbook(book)?))
            } else if event.get("price_changes").is_some() {
                // Parse price_changes message and convert to OrderBook updates
                let msg: PriceChangesMessage = serde_json::from_value(event.clone())?;
                price_changes_to_orderbook(msg)
            } else {
                Ok(None)
            }
        }
        _ => {
            tracing::trace!(event_type, "Unknown event type");
            Ok(None)
        }
    }
}

/// Convert a BookEvent to our OrderBook type
fn book_event_to_orderbook(book: BookEvent) -> anyhow::Result<OrderBook> {
    let bids: Vec<PriceLevel> = book
        .bids
        .into_iter()
        .filter_map(|level| {
            let price = Decimal::from_str(&level.price).ok()?;
            let size = Decimal::from_str(&level.size).ok()?;
            Some(PriceLevel { price, size })
        })
        .collect();

    let asks: Vec<PriceLevel> = book
        .asks
        .into_iter()
        .filter_map(|level| {
            let price = Decimal::from_str(&level.price).ok()?;
            let size = Decimal::from_str(&level.size).ok()?;
            Some(PriceLevel { price, size })
        })
        .collect();

    // Parse timestamp (milliseconds since epoch)
    let updated_at = if !book.timestamp.is_empty() {
        let millis: i64 = book.timestamp.parse().unwrap_or(0);
        Utc.timestamp_millis_opt(millis)
            .single()
            .unwrap_or_else(Utc::now)
    } else {
        Utc::now()
    };

    Ok(OrderBook {
        token_id: book.asset_id,
        bids,
        asks,
        updated_at,
    })
}

/// Convert price changes to OrderBook updates
///
/// Groups changes by asset_id and creates OrderBooks with the updated levels.
/// Returns the first asset's OrderBook (price_changes often contain multiple assets).
fn price_changes_to_orderbook(msg: PriceChangesMessage) -> anyhow::Result<Option<OrderBook>> {
    if msg.price_changes.is_empty() {
        return Ok(None);
    }

    // Group changes by asset_id
    let mut changes_by_asset: HashMap<String, (Vec<PriceLevel>, Vec<PriceLevel>)> = HashMap::new();

    for change in msg.price_changes {
        let price = match Decimal::from_str(&change.price) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let size = match Decimal::from_str(&change.size) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Skip zero-size levels (removals) - we only track current state
        if size.is_zero() {
            continue;
        }

        let level = PriceLevel { price, size };
        let entry = changes_by_asset
            .entry(change.asset_id.clone())
            .or_insert_with(|| (Vec::new(), Vec::new()));

        match change.side.as_str() {
            "BUY" => entry.0.push(level),  // bids
            "SELL" => entry.1.push(level), // asks
            _ => {}
        }
    }

    // Return first asset's order book
    if let Some((asset_id, (bids, asks))) = changes_by_asset.into_iter().next() {
        if bids.is_empty() && asks.is_empty() {
            return Ok(None);
        }

        tracing::trace!(
            token_id = %asset_id,
            bid_updates = bids.len(),
            ask_updates = asks.len(),
            "Parsed price changes"
        );

        Ok(Some(OrderBook {
            token_id: asset_id,
            bids,
            asks,
            updated_at: Utc::now(),
        }))
    } else {
        Ok(None)
    }
}

/// Order book manager for tracking multiple tokens
pub struct OrderBookManager {
    /// Current order books by token ID
    books: HashMap<String, OrderBook>,
}

impl OrderBookManager {
    /// Create a new order book manager
    pub fn new() -> Self {
        Self {
            books: HashMap::new(),
        }
    }

    /// Update or insert an order book (full snapshot replacement)
    pub fn update(&mut self, book: OrderBook) {
        // For full snapshots with many levels, replace entirely
        if book.bids.len() > 5 || book.asks.len() > 5 {
            self.books.insert(book.token_id.clone(), book);
        } else {
            // For small updates (likely incremental), merge with existing
            self.merge_update(book);
        }
    }

    /// Merge incremental updates into existing order book
    pub fn merge_update(&mut self, update: OrderBook) {
        if let Some(existing) = self.books.get_mut(&update.token_id) {
            // Merge bids: update existing levels or add new ones
            for new_level in update.bids {
                if let Some(pos) = existing.bids.iter().position(|l| l.price == new_level.price) {
                    existing.bids[pos].size = new_level.size;
                } else {
                    existing.bids.push(new_level);
                }
            }
            // Sort bids descending by price
            existing.bids.sort_by(|a, b| b.price.cmp(&a.price));

            // Merge asks: update existing levels or add new ones
            for new_level in update.asks {
                if let Some(pos) = existing.asks.iter().position(|l| l.price == new_level.price) {
                    existing.asks[pos].size = new_level.size;
                } else {
                    existing.asks.push(new_level);
                }
            }
            // Sort asks ascending by price
            existing.asks.sort_by(|a, b| a.price.cmp(&b.price));

            existing.updated_at = update.updated_at;
        } else {
            // No existing book, insert new one
            self.books.insert(update.token_id.clone(), update);
        }
    }

    /// Get an order book by token ID
    pub fn get(&self, token_id: &str) -> Option<&OrderBook> {
        self.books.get(token_id)
    }

    /// Get the best YES price (best ask on YES token)
    pub fn best_yes_price(&self, yes_token_id: &str) -> Option<Decimal> {
        self.books.get(yes_token_id).and_then(|b| b.best_ask())
    }

    /// Get the best NO price (best ask on NO token)
    pub fn best_no_price(&self, no_token_id: &str) -> Option<Decimal> {
        self.books.get(no_token_id).and_then(|b| b.best_ask())
    }

    /// Check if we have data for a token
    pub fn has_token(&self, token_id: &str) -> bool {
        self.books.contains_key(token_id)
    }

    /// Get number of tracked order books
    pub fn len(&self) -> usize {
        self.books.len()
    }

    /// Check if manager is empty
    pub fn is_empty(&self) -> bool {
        self.books.is_empty()
    }

    /// Clear all order books
    pub fn clear(&mut self) {
        self.books.clear();
    }
}

impl Default for OrderBookManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_polymarket_client_creation() {
        let client = PolymarketClient::new();
        assert_eq!(client.config.ws_url, POLYMARKET_WS_URL);
    }

    #[test]
    fn test_polymarket_config_default() {
        let config = PolymarketConfig::default();
        assert_eq!(config.max_reconnects, 0);
        assert_eq!(config.buffer_size, 256);
    }

    #[test]
    fn test_subscription_message_serialization() {
        let msg = SubscriptionMessage {
            assets_ids: vec!["token1".to_string(), "token2".to_string()],
            msg_type: "market".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"assets_ids\""));
        assert!(json.contains("\"type\":\"market\""));
        assert!(json.contains("token1"));
    }

    #[test]
    fn test_parse_book_event() {
        let json = r#"{
            "event_type": "book",
            "asset_id": "123456",
            "bids": [{"price": "0.50", "size": "100"}, {"price": "0.49", "size": "200"}],
            "asks": [{"price": "0.52", "size": "150"}, {"price": "0.53", "size": "250"}],
            "timestamp": "1704067200000",
            "hash": "abc123"
        }"#;

        let result = parse_market_message(json).unwrap();
        assert!(result.is_some());

        let book = result.unwrap();
        assert_eq!(book.token_id, "123456");
        assert_eq!(book.bids.len(), 2);
        assert_eq!(book.asks.len(), 2);
        assert_eq!(book.bids[0].price, dec!(0.50));
        assert_eq!(book.bids[0].size, dec!(100));
        assert_eq!(book.asks[0].price, dec!(0.52));
    }

    #[test]
    fn test_parse_book_event_without_event_type() {
        let json = r#"{
            "asset_id": "789",
            "bids": [{"price": "0.45", "size": "50"}],
            "asks": [{"price": "0.55", "size": "75"}]
        }"#;

        let result = parse_market_message(json).unwrap();
        assert!(result.is_some());

        let book = result.unwrap();
        assert_eq!(book.token_id, "789");
    }

    #[test]
    fn test_parse_price_change_event() {
        let json = r#"{
            "event_type": "price_change",
            "a": "123456",
            "p": "0.51",
            "s": "10",
            "si": "BUY",
            "bb": "0.50",
            "ba": "0.52"
        }"#;

        let result = parse_market_message(json).unwrap();
        // Price changes don't emit order books currently
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_event_list() {
        let json = r#"[
            {
                "event_type": "book",
                "asset_id": "111",
                "bids": [{"price": "0.40", "size": "100"}],
                "asks": [{"price": "0.60", "size": "100"}]
            }
        ]"#;

        let result = parse_market_message(json).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().token_id, "111");
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = parse_market_message("not valid json");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_book_level_parsing() {
        let book = BookEvent {
            asset_id: "test".to_string(),
            bids: vec![
                BookLevel {
                    price: "0.50".to_string(),
                    size: "100.5".to_string(),
                },
                BookLevel {
                    price: "invalid".to_string(),
                    size: "50".to_string(),
                },
            ],
            asks: vec![],
            hash: String::new(),
            timestamp: String::new(),
            market: String::new(),
        };

        let order_book = book_event_to_orderbook(book).unwrap();
        // Invalid price should be filtered out
        assert_eq!(order_book.bids.len(), 1);
        assert_eq!(order_book.bids[0].price, dec!(0.50));
        assert_eq!(order_book.bids[0].size, dec!(100.5));
    }

    #[test]
    fn test_order_book_manager_new() {
        let manager = OrderBookManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_order_book_manager_update() {
        let mut manager = OrderBookManager::new();

        let mut book = OrderBook::new("token1");
        book.bids = vec![PriceLevel {
            price: dec!(0.50),
            size: dec!(100),
        }];
        book.asks = vec![PriceLevel {
            price: dec!(0.52),
            size: dec!(100),
        }];

        manager.update(book);

        assert_eq!(manager.len(), 1);
        assert!(manager.has_token("token1"));
        assert!(!manager.has_token("token2"));
    }

    #[test]
    fn test_order_book_manager_get() {
        let mut manager = OrderBookManager::new();

        let mut book = OrderBook::new("yes_token");
        book.asks = vec![PriceLevel {
            price: dec!(0.55),
            size: dec!(200),
        }];

        manager.update(book);

        let retrieved = manager.get("yes_token");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().best_ask(), Some(dec!(0.55)));
    }

    #[test]
    fn test_order_book_manager_best_prices() {
        let mut manager = OrderBookManager::new();

        // YES token
        let mut yes_book = OrderBook::new("yes_token");
        yes_book.asks = vec![PriceLevel {
            price: dec!(0.52),
            size: dec!(100),
        }];
        manager.update(yes_book);

        // NO token
        let mut no_book = OrderBook::new("no_token");
        no_book.asks = vec![PriceLevel {
            price: dec!(0.48),
            size: dec!(100),
        }];
        manager.update(no_book);

        assert_eq!(manager.best_yes_price("yes_token"), Some(dec!(0.52)));
        assert_eq!(manager.best_no_price("no_token"), Some(dec!(0.48)));
        assert_eq!(manager.best_yes_price("missing"), None);
    }

    #[test]
    fn test_order_book_manager_clear() {
        let mut manager = OrderBookManager::new();
        manager.update(OrderBook::new("token1"));
        manager.update(OrderBook::new("token2"));

        assert_eq!(manager.len(), 2);

        manager.clear();
        assert!(manager.is_empty());
    }

    #[test]
    fn test_timestamp_parsing() {
        let book = BookEvent {
            asset_id: "test".to_string(),
            bids: vec![],
            asks: vec![],
            hash: String::new(),
            timestamp: "1704067200000".to_string(), // 2024-01-01 00:00:00 UTC
            market: String::new(),
        };

        let order_book = book_event_to_orderbook(book).unwrap();
        // Should parse the timestamp
        assert!(order_book.updated_at.timestamp() > 0);
    }

    #[test]
    fn test_empty_timestamp() {
        let book = BookEvent {
            asset_id: "test".to_string(),
            bids: vec![],
            asks: vec![],
            hash: String::new(),
            timestamp: String::new(),
            market: String::new(),
        };

        let order_book = book_event_to_orderbook(book).unwrap();
        // Should use current time
        assert!(order_book.updated_at.timestamp() > 0);
    }
}
