//! Fair value model module
//!
//! Calculates theoretical fair value for Yes/No tokens using GBM

mod gbm;
mod volatility;

pub use gbm::GbmModel;
pub use volatility::VolatilityEstimator;

use chrono::Duration;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Parameters for fair value calculation
#[derive(Debug, Clone)]
pub struct FairValueParams {
    /// Current spot price
    pub current_price: Decimal,
    /// Price at market open
    pub open_price: Decimal,
    /// Time to expiry
    pub time_to_expiry: Duration,
    /// Annualized volatility estimate
    pub volatility: Decimal,
}

/// Calculated fair value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FairValue {
    /// Fair probability of "Yes" outcome
    pub yes_prob: Decimal,
    /// Fair probability of "No" outcome
    pub no_prob: Decimal,
    /// Confidence level based on volatility certainty
    pub confidence: Decimal,
}

/// Trait for fair value model implementations
pub trait FairValueModel: Send + Sync {
    /// Calculate fair value given parameters
    fn calculate(&self, params: FairValueParams) -> FairValue;
}
