//! Lag detection module
//!
//! Detects when Polymarket odds are lagging behind confirmed spot price momentum.
//! This is the core strategy: enter AFTER momentum is confirmed but BEFORE odds catch up.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::config::LagConfig;
use crate::market::Market;
use crate::momentum::{MomentumDirection, MomentumSignal};

use super::types::{LagSignal, NoLagReason, OddsState, TradeSide};

/// Lag detector that compares momentum to current odds
///
/// The lag detector implements the core strategy logic:
/// 1. Receive confirmed momentum signal (BTC moved >0.7% from strike)
/// 2. Check current Polymarket odds
/// 3. If odds are still in neutral zone, there's a lag to exploit
/// 4. Generate trade signal with appropriate side and confidence
pub struct LagDetector {
    config: LagDetectorConfig,
}

/// Configuration for lag detection
#[derive(Debug, Clone)]
pub struct LagDetectorConfig {
    /// Minimum lag in cents to generate signal (e.g., 0.10 = 10 cents)
    pub min_lag_cents: Decimal,

    /// Maximum YES price for UP momentum (odds haven't caught up)
    pub max_yes_for_up: Decimal,

    /// Minimum YES price for DOWN momentum (odds haven't caught up)
    pub min_yes_for_down: Decimal,

    /// Minimum seconds after market open to trade
    pub min_seconds_after_open: i64,

    /// Maximum seconds before market close to trade
    pub max_seconds_before_close: i64,

    /// Expected price adjustment per 1% momentum move
    pub price_sensitivity: Decimal,
}

impl Default for LagDetectorConfig {
    fn default() -> Self {
        Self {
            min_lag_cents: dec!(0.10),     // 10 cents minimum lag
            max_yes_for_up: dec!(0.60),    // Don't buy YES if already > 60%
            min_yes_for_down: dec!(0.40),  // Don't buy NO if YES already < 40%
            min_seconds_after_open: 60,    // Wait 1 min after open
            max_seconds_before_close: 120, // Stop 2 min before close
            price_sensitivity: dec!(10),   // 1% move = ~10 cent expected change
        }
    }
}

impl From<&LagConfig> for LagDetectorConfig {
    fn from(config: &LagConfig) -> Self {
        Self {
            min_lag_cents: config.min_lag_cents,
            max_yes_for_up: config.max_yes_for_up,
            min_yes_for_down: config.min_yes_for_down,
            min_seconds_after_open: config.min_seconds_after_open as i64,
            max_seconds_before_close: config.max_seconds_before_close as i64,
            price_sensitivity: dec!(10),
        }
    }
}

impl LagDetector {
    /// Create a new lag detector with default configuration
    pub fn new() -> Self {
        Self {
            config: LagDetectorConfig::default(),
        }
    }

    /// Create a lag detector with custom configuration
    pub fn with_config(config: LagDetectorConfig) -> Self {
        Self { config }
    }

    /// Create from LagConfig
    pub fn from_lag_config(config: &LagConfig) -> Self {
        Self {
            config: LagDetectorConfig::from(config),
        }
    }

    /// Detect lag between momentum and current odds
    ///
    /// Returns Ok(Some(LagSignal)) if a trading opportunity is detected,
    /// Ok(None) if no opportunity exists, or Err with the reason.
    pub fn detect(
        &self,
        momentum: &MomentumSignal,
        odds: &OddsState,
        market: &Market,
    ) -> Result<Option<LagSignal>, NoLagReason> {
        let now = Utc::now();

        // Check time window
        let seconds_since_open = (now - market.open_time).num_seconds();
        let seconds_until_close = (market.close_time - now).num_seconds();

        // Too early check
        if seconds_since_open < self.config.min_seconds_after_open {
            return Err(NoLagReason::TooEarlyInWindow);
        }

        // Too close to close check
        if seconds_until_close < self.config.max_seconds_before_close {
            return Err(NoLagReason::TooCloseToClose);
        }

        // Market must be active
        if now < market.open_time || now > market.close_time {
            return Err(NoLagReason::MarketNotActive);
        }

        // Determine expected price based on momentum
        let expected_price = self.calculate_expected_price(momentum);

        // Check for lag based on momentum direction
        match momentum.direction {
            MomentumDirection::Up => {
                // For UP momentum, we want to buy YES
                // There's lag if YES price is still below our threshold
                if odds.yes_price >= self.config.max_yes_for_up {
                    return Err(NoLagReason::OddsAlreadyMoved);
                }

                let lag = expected_price - odds.yes_price;
                if lag < self.config.min_lag_cents {
                    return Err(NoLagReason::LagTooSmall);
                }

                Ok(Some(LagSignal::new(
                    TradeSide::Yes,
                    lag,
                    expected_price,
                    odds.yes_price,
                    momentum.clone(),
                    odds.clone(),
                    seconds_since_open,
                    seconds_until_close,
                )))
            }
            MomentumDirection::Down => {
                // For DOWN momentum, we want to buy NO
                // There's lag if YES price is still above our threshold
                if odds.yes_price <= self.config.min_yes_for_down {
                    return Err(NoLagReason::OddsAlreadyMoved);
                }

                // For down momentum, expected YES price is lower
                let expected_yes = Decimal::ONE - expected_price;
                let lag = odds.yes_price - expected_yes;

                if lag < self.config.min_lag_cents {
                    return Err(NoLagReason::LagTooSmall);
                }

                Ok(Some(LagSignal::new(
                    TradeSide::No,
                    lag,
                    expected_yes,
                    odds.yes_price,
                    momentum.clone(),
                    odds.clone(),
                    seconds_since_open,
                    seconds_until_close,
                )))
            }
        }
    }

    /// Detect lag with explicit timestamp (for testing/backtesting)
    pub fn detect_at(
        &self,
        momentum: &MomentumSignal,
        odds: &OddsState,
        market: &Market,
        now: DateTime<Utc>,
    ) -> Result<Option<LagSignal>, NoLagReason> {
        let seconds_since_open = (now - market.open_time).num_seconds();
        let seconds_until_close = (market.close_time - now).num_seconds();

        if seconds_since_open < self.config.min_seconds_after_open {
            return Err(NoLagReason::TooEarlyInWindow);
        }

        if seconds_until_close < self.config.max_seconds_before_close {
            return Err(NoLagReason::TooCloseToClose);
        }

        if now < market.open_time || now > market.close_time {
            return Err(NoLagReason::MarketNotActive);
        }

        let expected_price = self.calculate_expected_price(momentum);

        match momentum.direction {
            MomentumDirection::Up => {
                if odds.yes_price >= self.config.max_yes_for_up {
                    return Err(NoLagReason::OddsAlreadyMoved);
                }

                let lag = expected_price - odds.yes_price;
                if lag < self.config.min_lag_cents {
                    return Err(NoLagReason::LagTooSmall);
                }

                Ok(Some(LagSignal::new(
                    TradeSide::Yes,
                    lag,
                    expected_price,
                    odds.yes_price,
                    momentum.clone(),
                    odds.clone(),
                    seconds_since_open,
                    seconds_until_close,
                )))
            }
            MomentumDirection::Down => {
                if odds.yes_price <= self.config.min_yes_for_down {
                    return Err(NoLagReason::OddsAlreadyMoved);
                }

                let expected_yes = Decimal::ONE - expected_price;
                let lag = odds.yes_price - expected_yes;

                if lag < self.config.min_lag_cents {
                    return Err(NoLagReason::LagTooSmall);
                }

                Ok(Some(LagSignal::new(
                    TradeSide::No,
                    lag,
                    expected_yes,
                    odds.yes_price,
                    momentum.clone(),
                    odds.clone(),
                    seconds_since_open,
                    seconds_until_close,
                )))
            }
        }
    }

    /// Calculate expected price based on momentum
    ///
    /// Uses a simple linear model: base price (0.50) + sensitivity * move_pct
    /// For a 1% move with sensitivity=10, expected price = 0.50 + 0.10 = 0.60
    fn calculate_expected_price(&self, momentum: &MomentumSignal) -> Decimal {
        let base_price = dec!(0.50);
        let move_pct = momentum.move_pct;

        // Convert move percentage to price adjustment
        // move_pct is already a decimal (e.g., 0.01 for 1%)
        // Multiply by 100 to get percentage, then by sensitivity
        let adjustment = move_pct * dec!(100) * self.config.price_sensitivity / dec!(100);

        // Cap the expected price at reasonable bounds
        let expected = base_price + adjustment;
        expected.max(dec!(0.10)).min(dec!(0.90))
    }

    /// Get the configuration
    pub fn config(&self) -> &LagDetectorConfig {
        &self.config
    }
}

impl Default for LagDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn create_test_market() -> Market {
        let now = Utc::now();
        Market {
            condition_id: "test-condition".to_string(),
            yes_token_id: "yes-token".to_string(),
            no_token_id: "no-token".to_string(),
            open_price: dec!(95000),
            open_time: now - Duration::minutes(5), // Started 5 min ago
            close_time: now + Duration::minutes(10), // Closes in 10 min
        }
    }

    fn create_up_momentum() -> MomentumSignal {
        MomentumSignal::new(
            MomentumDirection::Up,
            dec!(0.01), // 1% move
            dec!(95000),
            dec!(95950),
            dec!(0.0001),
            dec!(0.8),
        )
    }

    fn create_down_momentum() -> MomentumSignal {
        MomentumSignal::new(
            MomentumDirection::Down,
            dec!(0.01), // 1% move
            dec!(95000),
            dec!(94050),
            dec!(0.0001),
            dec!(0.8),
        )
    }

    #[test]
    fn test_lag_detector_new() {
        let detector = LagDetector::new();
        assert_eq!(detector.config.min_lag_cents, dec!(0.10));
        assert_eq!(detector.config.max_yes_for_up, dec!(0.60));
    }

    #[test]
    fn test_calculate_expected_price_up() {
        let detector = LagDetector::new();
        let momentum = create_up_momentum();

        let expected = detector.calculate_expected_price(&momentum);
        // 1% move * 10 sensitivity = 10% adjustment
        // 0.50 + 0.10 = 0.60
        assert_eq!(expected, dec!(0.60));
    }

    #[test]
    fn test_calculate_expected_price_bounded() {
        let detector = LagDetector::new();

        // Large move should be capped
        let large_momentum = MomentumSignal::new(
            MomentumDirection::Up,
            dec!(0.05), // 5% move
            dec!(95000),
            dec!(99750),
            dec!(0.001),
            dec!(0.9),
        );

        let expected = detector.calculate_expected_price(&large_momentum);
        // Would be 0.50 + 0.50 = 1.00, but capped at 0.90
        assert_eq!(expected, dec!(0.90));
    }

    #[test]
    fn test_detect_up_momentum_with_lag() {
        let detector = LagDetector::new();
        let market = create_test_market();
        let momentum = create_up_momentum(); // 1% move -> expected 0.60
        let odds = OddsState::from_yes_price(dec!(0.48)); // Still at 0.48, lag = 0.12

        let now = market.open_time + Duration::minutes(3); // 3 min after open
        let result = detector.detect_at(&momentum, &odds, &market, now);

        assert!(result.is_ok());
        let signal = result.unwrap();
        assert!(signal.is_some());

        let signal = signal.unwrap();
        assert!(signal.is_yes());
        assert!(signal.lag_magnitude >= dec!(0.10)); // At least 10 cents lag
    }

    #[test]
    fn test_detect_down_momentum_with_lag() {
        let detector = LagDetector::new();
        let market = create_test_market();
        let momentum = create_down_momentum(); // 1% down move -> expected YES at 0.40
        let odds = OddsState::from_yes_price(dec!(0.52)); // Still at 0.52, lag = 0.12

        let now = market.open_time + Duration::minutes(3);
        let result = detector.detect_at(&momentum, &odds, &market, now);

        assert!(result.is_ok());
        let signal = result.unwrap();
        assert!(signal.is_some());

        let signal = signal.unwrap();
        assert!(signal.is_no());
        assert!(signal.lag_magnitude >= dec!(0.10));
    }

    #[test]
    fn test_detect_no_lag_odds_already_moved() {
        let detector = LagDetector::new();
        let market = create_test_market();
        let momentum = create_up_momentum();
        let odds = OddsState::from_yes_price(dec!(0.70)); // Already moved up

        let now = market.open_time + Duration::minutes(3);
        let result = detector.detect_at(&momentum, &odds, &market, now);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), NoLagReason::OddsAlreadyMoved);
    }

    #[test]
    fn test_detect_too_early_in_window() {
        let detector = LagDetector::new();
        let market = create_test_market();
        let momentum = create_up_momentum();
        let odds = OddsState::from_yes_price(dec!(0.52));

        // Only 30 seconds after open (need 60)
        let now = market.open_time + Duration::seconds(30);
        let result = detector.detect_at(&momentum, &odds, &market, now);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), NoLagReason::TooEarlyInWindow);
    }

    #[test]
    fn test_detect_too_close_to_close() {
        let detector = LagDetector::new();
        let market = create_test_market();
        let momentum = create_up_momentum();
        let odds = OddsState::from_yes_price(dec!(0.52));

        // Only 60 seconds before close (need 120)
        let now = market.close_time - Duration::seconds(60);
        let result = detector.detect_at(&momentum, &odds, &market, now);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), NoLagReason::TooCloseToClose);
    }

    #[test]
    fn test_detect_market_not_active() {
        let detector = LagDetector::new();
        let market = create_test_market();
        let momentum = create_up_momentum();
        let odds = OddsState::from_yes_price(dec!(0.52));

        // Before market opens
        let now = market.open_time - Duration::minutes(5);
        let result = detector.detect_at(&momentum, &odds, &market, now);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), NoLagReason::TooEarlyInWindow);
    }

    #[test]
    fn test_detect_lag_too_small() {
        let detector = LagDetector::with_config(LagDetectorConfig {
            min_lag_cents: dec!(0.20), // High threshold
            ..Default::default()
        });

        let market = create_test_market();
        // Small momentum = small expected price change
        let momentum = MomentumSignal::new(
            MomentumDirection::Up,
            dec!(0.007), // 0.7% move (minimum)
            dec!(95000),
            dec!(95665),
            dec!(0.00005),
            dec!(0.6),
        );
        let odds = OddsState::from_yes_price(dec!(0.55)); // Already close to expected

        let now = market.open_time + Duration::minutes(3);
        let result = detector.detect_at(&momentum, &odds, &market, now);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), NoLagReason::LagTooSmall);
    }

    #[test]
    fn test_config_from_lag_config() {
        let lag_config = LagConfig {
            min_lag_cents: dec!(0.15),
            max_yes_for_up: dec!(0.65),
            min_yes_for_down: dec!(0.35),
            min_seconds_after_open: 90,
            max_seconds_before_close: 180,
        };

        let detector = LagDetector::from_lag_config(&lag_config);
        assert_eq!(detector.config.min_lag_cents, dec!(0.15));
        assert_eq!(detector.config.max_yes_for_up, dec!(0.65));
        assert_eq!(detector.config.min_seconds_after_open, 90);
    }

    #[test]
    fn test_down_momentum_odds_already_moved() {
        let detector = LagDetector::new();
        let market = create_test_market();
        let momentum = create_down_momentum();
        let odds = OddsState::from_yes_price(dec!(0.30)); // Already moved down

        let now = market.open_time + Duration::minutes(3);
        let result = detector.detect_at(&momentum, &odds, &market, now);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), NoLagReason::OddsAlreadyMoved);
    }

    #[test]
    fn test_lag_signal_confidence() {
        let detector = LagDetector::new();
        let market = create_test_market();
        let momentum = create_up_momentum();
        let odds = OddsState::from_yes_price(dec!(0.45)); // Big lag

        let now = market.open_time + Duration::minutes(3);
        let result = detector.detect_at(&momentum, &odds, &market, now);

        let signal = result.unwrap().unwrap();
        // Larger lag = higher confidence
        assert!(signal.confidence > dec!(0.5));
    }
}
