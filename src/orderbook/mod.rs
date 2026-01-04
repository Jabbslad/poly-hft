//! Order book module
//!
//! Real-time order book from Polymarket WebSocket

mod book;
mod client;

pub use book::OrderBook;
pub use client::PolymarketClient;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A price level in the order book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    /// Price at this level
    pub price: Decimal,
    /// Total size available
    pub size: Decimal,
}
