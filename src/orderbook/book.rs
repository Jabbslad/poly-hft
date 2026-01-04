//! Order book state management

use super::PriceLevel;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// L2 aggregated order book for a token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    /// Token identifier
    pub token_id: String,
    /// Bid levels, sorted best (highest) to worst
    pub bids: Vec<PriceLevel>,
    /// Ask levels, sorted best (lowest) to worst
    pub asks: Vec<PriceLevel>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl OrderBook {
    /// Create a new empty order book
    pub fn new(token_id: impl Into<String>) -> Self {
        Self {
            token_id: token_id.into(),
            bids: vec![],
            asks: vec![],
            updated_at: Utc::now(),
        }
    }

    /// Get best bid price
    pub fn best_bid(&self) -> Option<Decimal> {
        self.bids.first().map(|l| l.price)
    }

    /// Get best ask price
    pub fn best_ask(&self) -> Option<Decimal> {
        self.asks.first().map(|l| l.price)
    }

    /// Get mid price
    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some((bid + ask) / Decimal::TWO),
            _ => None,
        }
    }

    /// Get spread
    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some(ask - bid),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_order_book_mid_price() {
        let mut book = OrderBook::new("test");
        book.bids = vec![PriceLevel {
            price: dec!(0.50),
            size: dec!(100),
        }];
        book.asks = vec![PriceLevel {
            price: dec!(0.52),
            size: dec!(100),
        }];

        assert_eq!(book.mid_price(), Some(dec!(0.51)));
        assert_eq!(book.spread(), Some(dec!(0.02)));
    }
}
