//! Spread capture strategy
//!
//! Detects arbitrage opportunities when buying both YES and NO
//! costs less than $1.00, guaranteeing profit regardless of outcome.
//!
//! Strategy: Buy both sides when combined cost < 1.00 - fees
//! Profit = 1.00 - (yes_ask + no_ask) - fees

use crate::market::Market;
use crate::orderbook::OrderBook;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Configuration for spread capture strategy
#[derive(Debug, Clone)]
pub struct SpreadConfig {
    /// Minimum profit percentage to trade (e.g., 0.02 = 2%)
    pub min_profit_pct: Decimal,
    /// Fee rate per side (e.g., 0.005 = 0.5%)
    pub fee_rate_per_side: Decimal,
    /// Maximum order book age in milliseconds
    pub max_book_age_ms: i64,
    /// Base position size in USD per leg
    pub base_size_usd: Decimal,
    /// Maximum concurrent spread positions
    pub max_positions: usize,
}

impl Default for SpreadConfig {
    fn default() -> Self {
        Self {
            min_profit_pct: dec!(0.02),     // 2% minimum profit
            fee_rate_per_side: dec!(0.005), // 0.5% per side
            max_book_age_ms: 2000,          // 2 second max staleness
            base_size_usd: dec!(5),         // $5 per leg
            max_positions: 50,
        }
    }
}

/// A spread capture opportunity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpreadSignal {
    /// Unique signal identifier
    pub id: Uuid,
    /// Associated market
    pub market: Market,
    /// YES side ask price
    pub yes_price: Decimal,
    /// NO side ask price
    pub no_price: Decimal,
    /// Total cost to buy both sides
    pub total_cost: Decimal,
    /// Gross profit (1.00 - total_cost)
    pub gross_profit: Decimal,
    /// Net profit after fees
    pub net_profit: Decimal,
    /// Profit percentage
    pub profit_pct: Decimal,
    /// Recommended size per leg in USD
    pub size_per_leg_usd: Decimal,
    /// Available liquidity on YES side
    pub yes_liquidity: Decimal,
    /// Available liquidity on NO side
    pub no_liquidity: Decimal,
    /// Signal generation timestamp
    pub timestamp: DateTime<Utc>,
}

impl SpreadSignal {
    /// Calculate expected dollar profit for given position size
    pub fn expected_profit_usd(&self, size_per_leg: Decimal) -> Decimal {
        // Each leg buys `size_per_leg` worth
        // Total outlay = size_per_leg (for YES) + size_per_leg (for NO)
        // But the profit comes from the price difference
        // If we spend $5 on YES at 0.56, we get 5/0.56 = 8.93 shares
        // If we spend $5 on NO at 0.42, we get 5/0.42 = 11.90 shares
        // Total cost = $10, but we need equal SHARES to guarantee profit
        // Better approach: buy X shares of each, cost = X * (yes + no)
        // Profit = X * (1.00 - yes - no) - fees
        self.net_profit * size_per_leg
    }
}

/// Market books container for both YES and NO sides
#[derive(Debug, Clone)]
pub struct MarketBooks {
    /// YES token order book
    pub yes_book: OrderBook,
    /// NO token order book
    pub no_book: OrderBook,
    /// Last update time (most recent of the two)
    pub updated_at: DateTime<Utc>,
}

impl MarketBooks {
    /// Create from two order books
    pub fn new(yes_book: OrderBook, no_book: OrderBook) -> Self {
        let updated_at = yes_book.updated_at.max(no_book.updated_at);
        Self {
            yes_book,
            no_book,
            updated_at,
        }
    }

    /// Get the age of the oldest book in milliseconds
    pub fn max_age_ms(&self) -> i64 {
        let now = Utc::now();
        let yes_age = (now - self.yes_book.updated_at).num_milliseconds();
        let no_age = (now - self.no_book.updated_at).num_milliseconds();
        yes_age.max(no_age)
    }

    /// Get YES ask price
    pub fn yes_ask(&self) -> Option<Decimal> {
        self.yes_book.best_ask()
    }

    /// Get NO ask price
    pub fn no_ask(&self) -> Option<Decimal> {
        self.no_book.best_ask()
    }

    /// Get YES ask size
    pub fn yes_ask_size(&self) -> Option<Decimal> {
        self.yes_book.best_ask_size()
    }

    /// Get NO ask size
    pub fn no_ask_size(&self) -> Option<Decimal> {
        self.no_book.best_ask_size()
    }

    /// Calculate combined cost to buy both sides
    pub fn combined_cost(&self) -> Option<Decimal> {
        match (self.yes_ask(), self.no_ask()) {
            (Some(yes), Some(no)) => Some(yes + no),
            _ => None,
        }
    }
}

/// Spread capture detector
#[derive(Debug)]
pub struct SpreadDetector {
    config: SpreadConfig,
    /// Track active positions by market condition_id
    active_positions: HashMap<String, usize>,
}

impl SpreadDetector {
    /// Create a new spread detector with default config
    pub fn new() -> Self {
        Self::with_config(SpreadConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: SpreadConfig) -> Self {
        Self {
            config,
            active_positions: HashMap::new(),
        }
    }

    /// Detect spread opportunity from market books
    pub fn detect(&self, market: &Market, books: &MarketBooks) -> Option<SpreadSignal> {
        // Check book freshness
        let age_ms = books.max_age_ms();
        if age_ms > self.config.max_book_age_ms {
            tracing::debug!(
                market = %market.condition_id,
                age_ms = age_ms,
                max_age_ms = self.config.max_book_age_ms,
                "Order books too stale"
            );
            return None;
        }

        // Get prices
        let yes_ask = books.yes_ask()?;
        let no_ask = books.no_ask()?;
        let total_cost = yes_ask + no_ask;

        // Calculate gross profit
        let gross_profit = Decimal::ONE - total_cost;
        if gross_profit <= Decimal::ZERO {
            tracing::debug!(
                market = %market.condition_id,
                yes_ask = %yes_ask,
                no_ask = %no_ask,
                total_cost = %total_cost,
                "No spread profit available"
            );
            return None;
        }

        // Calculate net profit after fees (both sides)
        let total_fees = self.config.fee_rate_per_side * dec!(2);
        let net_profit = gross_profit - total_fees;

        if net_profit <= Decimal::ZERO {
            tracing::debug!(
                market = %market.condition_id,
                gross_profit = %gross_profit,
                fees = %total_fees,
                "Spread profit eaten by fees"
            );
            return None;
        }

        // Check minimum profit threshold
        let profit_pct = net_profit / total_cost;
        if profit_pct < self.config.min_profit_pct {
            tracing::debug!(
                market = %market.condition_id,
                profit_pct = %profit_pct,
                min_pct = %self.config.min_profit_pct,
                "Profit below threshold"
            );
            return None;
        }

        // Get liquidity
        let yes_liquidity = books.yes_ask_size().unwrap_or(Decimal::ZERO);
        let no_liquidity = books.no_ask_size().unwrap_or(Decimal::ZERO);

        // Determine position size (limited by liquidity)
        let min_liquidity = yes_liquidity.min(no_liquidity);
        let size_per_leg = self.config.base_size_usd.min(min_liquidity);

        if size_per_leg < dec!(1) {
            tracing::debug!(
                market = %market.condition_id,
                yes_liquidity = %yes_liquidity,
                no_liquidity = %no_liquidity,
                "Insufficient liquidity"
            );
            return None;
        }

        tracing::info!(
            market = %market.condition_id,
            yes_ask = %yes_ask,
            no_ask = %no_ask,
            total_cost = %total_cost,
            gross_profit = %gross_profit,
            net_profit = %net_profit,
            profit_pct = %profit_pct,
            size_per_leg = %size_per_leg,
            "Spread opportunity detected!"
        );

        Some(SpreadSignal {
            id: Uuid::new_v4(),
            market: market.clone(),
            yes_price: yes_ask,
            no_price: no_ask,
            total_cost,
            gross_profit,
            net_profit,
            profit_pct,
            size_per_leg_usd: size_per_leg,
            yes_liquidity,
            no_liquidity,
            timestamp: Utc::now(),
        })
    }

    /// Record a new position for a market
    pub fn add_position(&mut self, market_id: &str) {
        *self
            .active_positions
            .entry(market_id.to_string())
            .or_insert(0) += 1;
    }

    /// Remove a position for a market
    pub fn remove_position(&mut self, market_id: &str) {
        if let Some(count) = self.active_positions.get_mut(market_id) {
            if *count > 0 {
                *count -= 1;
            }
        }
    }

    /// Check if we can take more positions on a market
    pub fn can_take_position(&self, market_id: &str) -> bool {
        let current = self.active_positions.get(market_id).unwrap_or(&0);
        *current < self.config.max_positions
    }

    /// Get the config
    pub fn config(&self) -> &SpreadConfig {
        &self.config
    }
}

impl Default for SpreadDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orderbook::PriceLevel;
    use chrono::Duration;

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
            bids: vec![],
            asks: vec![PriceLevel {
                price: ask_price,
                size: ask_size,
            }],
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_spread_detector_creation() {
        let detector = SpreadDetector::new();
        assert_eq!(detector.config.min_profit_pct, dec!(0.02));
    }

    #[test]
    fn test_detect_profitable_spread() {
        let config = SpreadConfig {
            min_profit_pct: dec!(0.01), // 1% minimum
            fee_rate_per_side: dec!(0.005),
            max_book_age_ms: 5000,
            base_size_usd: dec!(10),
            max_positions: 10,
        };
        let detector = SpreadDetector::with_config(config);
        let market = create_test_market();

        // YES @ 0.56, NO @ 0.40 = 0.96 total, 4% gross profit
        let yes_book = create_orderbook("yes-token", dec!(0.56), dec!(100));
        let no_book = create_orderbook("no-token", dec!(0.40), dec!(100));
        let books = MarketBooks::new(yes_book, no_book);

        let signal = detector.detect(&market, &books);
        assert!(signal.is_some());

        let s = signal.unwrap();
        assert_eq!(s.yes_price, dec!(0.56));
        assert_eq!(s.no_price, dec!(0.40));
        assert_eq!(s.total_cost, dec!(0.96));
        assert_eq!(s.gross_profit, dec!(0.04));
        // Net profit = 0.04 - 0.01 (fees) = 0.03
        assert_eq!(s.net_profit, dec!(0.03));
    }

    #[test]
    fn test_detect_no_spread_profit() {
        let detector = SpreadDetector::new();
        let market = create_test_market();

        // YES @ 0.55, NO @ 0.50 = 1.05 total, no profit
        let yes_book = create_orderbook("yes-token", dec!(0.55), dec!(100));
        let no_book = create_orderbook("no-token", dec!(0.50), dec!(100));
        let books = MarketBooks::new(yes_book, no_book);

        let signal = detector.detect(&market, &books);
        assert!(signal.is_none());
    }

    #[test]
    fn test_detect_profit_below_threshold() {
        let config = SpreadConfig {
            min_profit_pct: dec!(0.05), // 5% minimum
            fee_rate_per_side: dec!(0.005),
            max_book_age_ms: 5000,
            base_size_usd: dec!(10),
            max_positions: 10,
        };
        let detector = SpreadDetector::with_config(config);
        let market = create_test_market();

        // YES @ 0.56, NO @ 0.42 = 0.98 total, 2% gross profit
        // Below 5% threshold
        let yes_book = create_orderbook("yes-token", dec!(0.56), dec!(100));
        let no_book = create_orderbook("no-token", dec!(0.42), dec!(100));
        let books = MarketBooks::new(yes_book, no_book);

        let signal = detector.detect(&market, &books);
        assert!(signal.is_none());
    }

    #[test]
    fn test_detect_stale_books() {
        let config = SpreadConfig {
            max_book_age_ms: 100, // Very short
            ..Default::default()
        };
        let detector = SpreadDetector::with_config(config);
        let market = create_test_market();

        // Create stale books
        let mut yes_book = create_orderbook("yes-token", dec!(0.56), dec!(100));
        yes_book.updated_at = Utc::now() - Duration::seconds(5);
        let no_book = create_orderbook("no-token", dec!(0.40), dec!(100));
        let books = MarketBooks::new(yes_book, no_book);

        let signal = detector.detect(&market, &books);
        assert!(signal.is_none());
    }

    #[test]
    fn test_detect_insufficient_liquidity() {
        let detector = SpreadDetector::new();
        let market = create_test_market();

        // Very low liquidity
        let yes_book = create_orderbook("yes-token", dec!(0.56), dec!(0.5));
        let no_book = create_orderbook("no-token", dec!(0.40), dec!(0.5));
        let books = MarketBooks::new(yes_book, no_book);

        let signal = detector.detect(&market, &books);
        assert!(signal.is_none());
    }

    #[test]
    fn test_position_tracking() {
        let mut detector = SpreadDetector::new();

        assert!(detector.can_take_position("market-1"));

        detector.add_position("market-1");
        assert!(detector.can_take_position("market-1"));

        // Fill up to max
        for _ in 0..49 {
            detector.add_position("market-1");
        }
        assert!(!detector.can_take_position("market-1"));

        detector.remove_position("market-1");
        assert!(detector.can_take_position("market-1"));
    }

    #[test]
    fn test_market_books_combined_cost() {
        let yes_book = create_orderbook("yes", dec!(0.60), dec!(100));
        let no_book = create_orderbook("no", dec!(0.38), dec!(100));
        let books = MarketBooks::new(yes_book, no_book);

        assert_eq!(books.combined_cost(), Some(dec!(0.98)));
    }
}
