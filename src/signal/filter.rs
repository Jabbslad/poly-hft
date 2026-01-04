//! Signal filtering

use super::Signal;
use chrono::Duration;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Result of applying filters to a signal
#[derive(Debug, Clone)]
pub enum FilterResult {
    /// Signal passed all filters
    Pass,
    /// Signal rejected
    Reject(RejectReason),
}

/// Reason for signal rejection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RejectReason {
    /// Edge below minimum threshold
    EdgeTooSmall(Decimal),
    /// Edge above maximum threshold (likely stale data)
    EdgeTooLarge(Decimal),
    /// Insufficient liquidity at target price
    InsufficientLiquidity(Decimal),
    /// Too close to market expiry
    TooCloseToExpiry(Duration),
    /// Volatility estimate out of reasonable range
    VolatilityOutOfRange(Decimal),
    /// Maximum concurrent positions reached
    MaxPositionsReached,
}

/// Configuration for signal filters
#[derive(Debug, Clone)]
pub struct FilterConfig {
    /// Minimum adjusted edge threshold
    pub min_edge: Decimal,
    /// Maximum adjusted edge threshold
    pub max_edge: Decimal,
    /// Minimum time to expiry
    pub min_time_to_expiry: Duration,
    /// Maximum time to expiry (from market open)
    pub max_time_to_expiry: Duration,
    /// Minimum order book liquidity
    pub min_liquidity: Decimal,
    /// Minimum volatility (annualized)
    pub min_volatility: Decimal,
    /// Maximum volatility (annualized)
    pub max_volatility: Decimal,
}

/// Signal filter chain
pub struct SignalFilter {
    config: FilterConfig,
}

impl SignalFilter {
    /// Create a new signal filter with given configuration
    pub fn new(config: FilterConfig) -> Self {
        Self { config }
    }

    /// Apply all filters to a signal
    pub fn apply(
        &self,
        signal: &Signal,
        current_positions: usize,
        max_positions: usize,
        available_liquidity: Decimal,
        volatility: Decimal,
        time_to_expiry: Duration,
    ) -> FilterResult {
        // Check position limits
        if current_positions >= max_positions {
            return FilterResult::Reject(RejectReason::MaxPositionsReached);
        }

        // Check edge thresholds
        if signal.adjusted_edge < self.config.min_edge {
            return FilterResult::Reject(RejectReason::EdgeTooSmall(signal.adjusted_edge));
        }
        if signal.adjusted_edge > self.config.max_edge {
            return FilterResult::Reject(RejectReason::EdgeTooLarge(signal.adjusted_edge));
        }

        // Check time to expiry
        if time_to_expiry < self.config.min_time_to_expiry {
            return FilterResult::Reject(RejectReason::TooCloseToExpiry(time_to_expiry));
        }

        // Check liquidity
        if available_liquidity < self.config.min_liquidity {
            return FilterResult::Reject(RejectReason::InsufficientLiquidity(available_liquidity));
        }

        // Check volatility
        if volatility < self.config.min_volatility || volatility > self.config.max_volatility {
            return FilterResult::Reject(RejectReason::VolatilityOutOfRange(volatility));
        }

        FilterResult::Pass
    }
}
