//! Spread capture orchestrator
//!
//! Manages the spread capture strategy by:
//! 1. Tracking both YES and NO order books for each market
//! 2. Detecting spread opportunities
//! 3. Generating dual-sided trade signals

use super::spread::{MarketBooks, SpreadConfig, SpreadDetector, SpreadSignal};
use crate::market::{Market, MarketTracker};
use crate::orderbook::OrderBook;
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Spread orchestrator state
struct OrchestratorState {
    /// Order books by token_id
    order_books: HashMap<String, OrderBook>,
    /// Spread detector
    detector: SpreadDetector,
}

impl OrchestratorState {
    fn new(config: SpreadConfig) -> Self {
        Self {
            order_books: HashMap::new(),
            detector: SpreadDetector::with_config(config),
        }
    }
}

/// Spread capture orchestrator
pub struct SpreadOrchestrator<T: MarketTracker> {
    market_tracker: Arc<T>,
    state: Arc<RwLock<OrchestratorState>>,
    /// Interval between spread checks in milliseconds
    check_interval_ms: u64,
}

impl<T: MarketTracker + Send + Sync + 'static> SpreadOrchestrator<T> {
    /// Create a new spread orchestrator
    pub fn new(market_tracker: Arc<T>, config: SpreadConfig) -> Self {
        Self {
            market_tracker,
            state: Arc::new(RwLock::new(OrchestratorState::new(config))),
            check_interval_ms: 100, // Check every 100ms
        }
    }

    /// Create with default config
    pub fn with_defaults(market_tracker: Arc<T>) -> Self {
        Self::new(market_tracker, SpreadConfig::default())
    }

    /// Update order book
    pub async fn update_order_book(&self, book: OrderBook) {
        let mut state = self.state.write().await;
        state.order_books.insert(book.token_id.clone(), book);
    }

    /// Get MarketBooks for a market if both YES and NO books exist
    async fn get_market_books(&self, market: &Market) -> Option<MarketBooks> {
        let state = self.state.read().await;

        let yes_book = state.order_books.get(&market.yes_token_id)?.clone();
        let no_book = state.order_books.get(&market.no_token_id)?.clone();

        Some(MarketBooks::new(yes_book, no_book))
    }

    /// Check for spread opportunities on all active markets
    pub async fn check_spreads(&self) -> Result<Vec<SpreadSignal>> {
        let markets = self.market_tracker.get_active_markets().await?;
        let mut signals = Vec::new();

        for market in markets {
            // Skip expired markets
            if market.close_time <= Utc::now() {
                continue;
            }

            // Get both order books
            let books = match self.get_market_books(&market).await {
                Some(b) => b,
                None => {
                    tracing::debug!(
                        market = %market.condition_id,
                        "Missing YES or NO order book"
                    );
                    continue;
                }
            };

            // Check for spread opportunity
            let state = self.state.read().await;
            if !state.detector.can_take_position(&market.condition_id) {
                tracing::debug!(
                    market = %market.condition_id,
                    "Max positions reached for market"
                );
                continue;
            }

            if let Some(signal) = state.detector.detect(&market, &books) {
                signals.push(signal);
            }
        }

        Ok(signals)
    }

    /// Run the orchestrator with input channel for order books
    pub async fn run(
        self: Arc<Self>,
        mut book_rx: mpsc::Receiver<OrderBook>,
    ) -> Result<mpsc::Receiver<SpreadSignal>> {
        let (signal_tx, signal_rx) = mpsc::channel(64);
        let orchestrator = Arc::clone(&self);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(
                orchestrator.check_interval_ms,
            ));

            loop {
                tokio::select! {
                    // Handle order book updates
                    Some(book) = book_rx.recv() => {
                        orchestrator.update_order_book(book).await;
                    }

                    // Periodic spread check
                    _ = interval.tick() => {
                        match orchestrator.check_spreads().await {
                            Ok(signals) => {
                                for signal in signals {
                                    if signal_tx.send(signal).await.is_err() {
                                        tracing::warn!("Signal receiver dropped");
                                        return;
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Spread check failed");
                            }
                        }
                    }
                }
            }
        });

        Ok(signal_rx)
    }

    /// Record that we took a position
    pub async fn record_position(&self, market_id: &str) {
        let mut state = self.state.write().await;
        state.detector.add_position(market_id);
    }

    /// Record that we closed a position
    pub async fn close_position(&self, market_id: &str) {
        let mut state = self.state.write().await;
        state.detector.remove_position(market_id);
    }

    /// Get current spread config
    pub async fn config(&self) -> SpreadConfig {
        let state = self.state.read().await;
        state.detector.config().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orderbook::PriceLevel;
    use async_trait::async_trait;
    use chrono::Duration;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    struct MockMarketTracker {
        markets: RwLock<Vec<Market>>,
    }

    impl MockMarketTracker {
        fn new() -> Self {
            Self {
                markets: RwLock::new(vec![]),
            }
        }

        async fn add_market(&self, market: Market) {
            let mut markets = self.markets.write().await;
            markets.push(market);
        }
    }

    #[async_trait]
    impl MarketTracker for MockMarketTracker {
        async fn get_active_markets(&self) -> Result<Vec<Market>> {
            let markets = self.markets.read().await;
            Ok(markets.clone())
        }

        async fn refresh(&self) -> Result<()> {
            Ok(())
        }
    }

    fn create_test_market() -> Market {
        let now = Utc::now();
        Market {
            condition_id: "test-market".to_string(),
            yes_token_id: "yes-token".to_string(),
            no_token_id: "no-token".to_string(),
            open_price: dec!(90000),
            open_time: now - Duration::minutes(5),
            close_time: now + Duration::minutes(10),
        }
    }

    fn create_orderbook(token_id: &str, ask_price: Decimal, ask_size: Decimal) -> OrderBook {
        OrderBook {
            token_id: token_id.to_string(),
            bids: vec![PriceLevel {
                price: ask_price - dec!(0.02),
                size: ask_size,
            }],
            asks: vec![PriceLevel {
                price: ask_price,
                size: ask_size,
            }],
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let tracker = Arc::new(MockMarketTracker::new());
        let _orchestrator = SpreadOrchestrator::with_defaults(tracker);
    }

    #[tokio::test]
    async fn test_update_order_book() {
        let tracker = Arc::new(MockMarketTracker::new());
        let orchestrator = SpreadOrchestrator::with_defaults(tracker);

        let book = create_orderbook("yes-token", dec!(0.56), dec!(100));
        orchestrator.update_order_book(book).await;

        let state = orchestrator.state.read().await;
        assert!(state.order_books.contains_key("yes-token"));
    }

    #[tokio::test]
    async fn test_check_spreads_with_opportunity() {
        let tracker = Arc::new(MockMarketTracker::new());
        let market = create_test_market();
        tracker.add_market(market).await;

        let config = SpreadConfig {
            min_profit_pct: dec!(0.01),
            fee_rate_per_side: dec!(0.005),
            max_book_age_ms: 5000,
            base_size_usd: dec!(10),
            max_positions: 10,
        };
        let orchestrator = SpreadOrchestrator::new(tracker, config);

        // Add both books with profitable spread
        let yes_book = create_orderbook("yes-token", dec!(0.56), dec!(100));
        let no_book = create_orderbook("no-token", dec!(0.40), dec!(100));
        orchestrator.update_order_book(yes_book).await;
        orchestrator.update_order_book(no_book).await;

        let signals = orchestrator.check_spreads().await.unwrap();
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].yes_price, dec!(0.56));
        assert_eq!(signals[0].no_price, dec!(0.40));
    }

    #[tokio::test]
    async fn test_check_spreads_missing_book() {
        let tracker = Arc::new(MockMarketTracker::new());
        let market = create_test_market();
        tracker.add_market(market).await;

        let orchestrator = SpreadOrchestrator::with_defaults(tracker);

        // Only add YES book
        let yes_book = create_orderbook("yes-token", dec!(0.56), dec!(100));
        orchestrator.update_order_book(yes_book).await;

        let signals = orchestrator.check_spreads().await.unwrap();
        assert!(signals.is_empty());
    }

    #[tokio::test]
    async fn test_position_tracking() {
        let tracker = Arc::new(MockMarketTracker::new());
        let orchestrator = SpreadOrchestrator::with_defaults(tracker);

        orchestrator.record_position("market-1").await;

        let state = orchestrator.state.read().await;
        assert!(state.detector.can_take_position("market-1"));
    }
}
