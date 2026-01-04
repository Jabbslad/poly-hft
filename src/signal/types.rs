//! Signal types

use crate::market::Market;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Trading side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// Buy Yes tokens
    Yes,
    /// Buy No tokens
    No,
}

/// Reason for signal generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalReason {
    /// Market just opened, prices lagging
    PostResetLag,
    /// Spot moved significantly, odds stale
    SpotDivergence,
    /// Volatility increased, fair value shifted
    VolatilitySpike,
    /// Spread capture - buying both sides for guaranteed profit
    SpreadCapture,
}

/// A trading signal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    /// Unique signal identifier
    pub id: Uuid,
    /// Associated market
    pub market: Market,
    /// Trade direction
    pub side: Side,
    /// Calculated fair value
    pub fair_value: Decimal,
    /// Current market price
    pub market_price: Decimal,
    /// Raw edge before costs
    pub raw_edge: Decimal,
    /// Adjusted edge after fees/slippage
    pub adjusted_edge: Decimal,
    /// Confidence score
    pub confidence: Decimal,
    /// Reason for signal
    pub reason: SignalReason,
    /// Signal generation timestamp
    pub timestamp: DateTime<Utc>,
}

impl Signal {
    /// Create a new signal
    pub fn new(
        market: Market,
        side: Side,
        fair_value: Decimal,
        market_price: Decimal,
        adjusted_edge: Decimal,
        confidence: Decimal,
        reason: SignalReason,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            market,
            side,
            fair_value,
            market_price,
            raw_edge: fair_value - market_price,
            adjusted_edge,
            confidence,
            reason,
            timestamp: Utc::now(),
        }
    }
}
