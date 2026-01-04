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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::Market;
    use crate::signal::{Side, SignalReason};
    use chrono::Utc;
    use rust_decimal_macros::dec;

    fn default_filter_config() -> FilterConfig {
        FilterConfig {
            min_edge: dec!(0.005),
            max_edge: dec!(0.15),
            min_time_to_expiry: Duration::minutes(1),
            max_time_to_expiry: Duration::minutes(14),
            min_liquidity: dec!(100),
            min_volatility: dec!(0.1),
            max_volatility: dec!(1.5),
        }
    }

    fn create_test_signal(adjusted_edge: Decimal) -> Signal {
        let market = Market {
            condition_id: "test-cond".to_string(),
            yes_token_id: "yes-token".to_string(),
            no_token_id: "no-token".to_string(),
            open_price: dec!(100000),
            open_time: Utc::now() - Duration::minutes(5),
            close_time: Utc::now() + Duration::minutes(10),
        };

        Signal::new(
            market,
            Side::Yes,
            dec!(0.55),
            dec!(0.50),
            adjusted_edge,
            dec!(0.8),
            SignalReason::SpotDivergence,
        )
    }

    #[test]
    fn test_filter_creation() {
        let config = default_filter_config();
        let filter = SignalFilter::new(config);
        assert_eq!(filter.config.min_edge, dec!(0.005));
    }

    #[test]
    fn test_filter_pass() {
        let config = default_filter_config();
        let filter = SignalFilter::new(config);
        let signal = create_test_signal(dec!(0.02));

        let result = filter.apply(
            &signal,
            0,                     // current positions
            5,                     // max positions
            dec!(500),             // available liquidity
            dec!(0.4),             // volatility
            Duration::minutes(10), // time to expiry
        );

        assert!(matches!(result, FilterResult::Pass));
    }

    #[test]
    fn test_filter_reject_max_positions() {
        let config = default_filter_config();
        let filter = SignalFilter::new(config);
        let signal = create_test_signal(dec!(0.02));

        let result = filter.apply(
            &signal,
            5, // current positions == max
            5, // max positions
            dec!(500),
            dec!(0.4),
            Duration::minutes(10),
        );

        assert!(matches!(
            result,
            FilterResult::Reject(RejectReason::MaxPositionsReached)
        ));
    }

    #[test]
    fn test_filter_reject_edge_too_small() {
        let config = default_filter_config();
        let filter = SignalFilter::new(config);
        let signal = create_test_signal(dec!(0.001)); // Below min_edge

        let result = filter.apply(&signal, 0, 5, dec!(500), dec!(0.4), Duration::minutes(10));

        assert!(matches!(
            result,
            FilterResult::Reject(RejectReason::EdgeTooSmall(_))
        ));
    }

    #[test]
    fn test_filter_reject_edge_too_large() {
        let config = default_filter_config();
        let filter = SignalFilter::new(config);
        let signal = create_test_signal(dec!(0.20)); // Above max_edge

        let result = filter.apply(&signal, 0, 5, dec!(500), dec!(0.4), Duration::minutes(10));

        assert!(matches!(
            result,
            FilterResult::Reject(RejectReason::EdgeTooLarge(_))
        ));
    }

    #[test]
    fn test_filter_reject_time_to_expiry() {
        let config = default_filter_config();
        let filter = SignalFilter::new(config);
        let signal = create_test_signal(dec!(0.02));

        let result = filter.apply(
            &signal,
            0,
            5,
            dec!(500),
            dec!(0.4),
            Duration::seconds(30), // Below min_time_to_expiry
        );

        assert!(matches!(
            result,
            FilterResult::Reject(RejectReason::TooCloseToExpiry(_))
        ));
    }

    #[test]
    fn test_filter_reject_insufficient_liquidity() {
        let config = default_filter_config();
        let filter = SignalFilter::new(config);
        let signal = create_test_signal(dec!(0.02));

        let result = filter.apply(
            &signal,
            0,
            5,
            dec!(50), // Below min_liquidity
            dec!(0.4),
            Duration::minutes(10),
        );

        assert!(matches!(
            result,
            FilterResult::Reject(RejectReason::InsufficientLiquidity(_))
        ));
    }

    #[test]
    fn test_filter_reject_volatility_too_low() {
        let config = default_filter_config();
        let filter = SignalFilter::new(config);
        let signal = create_test_signal(dec!(0.02));

        let result = filter.apply(
            &signal,
            0,
            5,
            dec!(500),
            dec!(0.05), // Below min_volatility
            Duration::minutes(10),
        );

        assert!(matches!(
            result,
            FilterResult::Reject(RejectReason::VolatilityOutOfRange(_))
        ));
    }

    #[test]
    fn test_filter_reject_volatility_too_high() {
        let config = default_filter_config();
        let filter = SignalFilter::new(config);
        let signal = create_test_signal(dec!(0.02));

        let result = filter.apply(
            &signal,
            0,
            5,
            dec!(500),
            dec!(2.0), // Above max_volatility
            Duration::minutes(10),
        );

        assert!(matches!(
            result,
            FilterResult::Reject(RejectReason::VolatilityOutOfRange(_))
        ));
    }

    #[test]
    fn test_reject_reason_display_edge_too_small() {
        let reason = RejectReason::EdgeTooSmall(dec!(0.001));
        let serialized = serde_json::to_string(&reason).unwrap();
        assert!(serialized.contains("EdgeTooSmall"));
    }

    #[test]
    fn test_filter_config_clone() {
        let config = default_filter_config();
        let cloned = config.clone();
        assert_eq!(config.min_edge, cloned.min_edge);
        assert_eq!(config.max_edge, cloned.max_edge);
    }
}
