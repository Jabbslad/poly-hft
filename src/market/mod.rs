//! Market discovery module
//!
//! Finds and tracks active 15-minute BTC up/down markets via Gamma API

mod gamma;
mod tracker;

pub use gamma::GammaClient;
pub use tracker::MarketTrackerImpl;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A Polymarket 15-minute binary market
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    /// Unique condition identifier
    pub condition_id: String,
    /// Yes token identifier
    pub yes_token_id: String,
    /// No token identifier
    pub no_token_id: String,
    /// BTC price at market open
    pub open_price: Decimal,
    /// Market open time
    pub open_time: DateTime<Utc>,
    /// Market close/settlement time
    pub close_time: DateTime<Utc>,
}

/// Trait for market tracking implementations
#[async_trait]
pub trait MarketTracker: Send + Sync {
    /// Get currently active markets
    async fn get_active_markets(&self) -> anyhow::Result<Vec<Market>>;
    /// Refresh market list from API
    async fn refresh(&self) -> anyhow::Result<()>;
}
