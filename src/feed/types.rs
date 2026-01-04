//! Price feed types

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A single price tick from an exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceTick {
    /// Trading symbol (e.g., "BTCUSDT")
    pub symbol: String,
    /// Trade price
    pub price: Decimal,
    /// Local timestamp when tick was received
    pub timestamp: DateTime<Utc>,
    /// Exchange timestamp (e.g., Binance event time)
    pub exchange_ts: DateTime<Utc>,
}
