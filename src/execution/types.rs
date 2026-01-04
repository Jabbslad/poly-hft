//! Execution types

use crate::signal::Side;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Order identifier
pub type OrderId = Uuid;

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    /// Market order (immediate execution)
    Market,
    /// Limit order (price specified)
    Limit,
}

/// An order to be submitted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    /// Token identifier
    pub token_id: String,
    /// Trade side
    pub side: Side,
    /// Order price (for limit orders)
    pub price: Decimal,
    /// Order size
    pub size: Decimal,
    /// Order type
    pub order_type: OrderType,
}

/// A fill (executed trade)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    /// Order ID
    pub order_id: OrderId,
    /// Token ID
    pub token_id: String,
    /// Trade side
    pub side: Side,
    /// Fill price
    pub price: Decimal,
    /// Fill size
    pub size: Decimal,
    /// Fill timestamp
    pub timestamp: DateTime<Utc>,
    /// Fees paid
    pub fees: Decimal,
}
