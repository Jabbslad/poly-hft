//! Position sizing implementations
//!
//! Provides fixed and Kelly-based position sizing for the lag edges strategy.
//! Fixed sizing is preferred for the lag edges approach due to high win rate.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::config::SizingConfig;
use crate::lag::LagSignal;

/// Trait for position sizing implementations
pub trait PositionSizer: Send + Sync {
    /// Calculate position size in dollars for a given signal and bankroll
    fn calculate(&self, signal: &LagSignal, bankroll: Decimal) -> Decimal;

    /// Get the sizing mode name
    fn mode_name(&self) -> &'static str;
}

/// Fixed percentage position sizing
///
/// Uses a fixed percentage of the bankroll for each trade.
/// This is the recommended approach for lag edges due to:
/// - High win rate (80-95% expected)
/// - Consistent execution
/// - Simple risk management
#[derive(Debug, Clone)]
pub struct FixedSizer {
    /// Fixed percentage of bankroll per trade (e.g., 0.10 = 10%)
    pub fixed_pct: Decimal,
    /// Maximum percentage cap (e.g., 0.20 = 20%)
    pub max_pct: Decimal,
    /// Minimum trade size in dollars
    pub min_size: Decimal,
}

impl FixedSizer {
    /// Create a new fixed sizer
    pub fn new(fixed_pct: Decimal, max_pct: Decimal) -> Self {
        Self {
            fixed_pct,
            max_pct,
            min_size: dec!(1), // $1 minimum
        }
    }

    /// Create from SizingConfig
    pub fn from_config(config: &SizingConfig) -> Self {
        Self {
            fixed_pct: config.fixed_pct,
            max_pct: config.max_pct,
            min_size: dec!(1),
        }
    }

    /// Set minimum trade size
    pub fn with_min_size(mut self, min_size: Decimal) -> Self {
        self.min_size = min_size;
        self
    }

    /// Calculate position size
    pub fn calculate_size(&self, bankroll: Decimal) -> Decimal {
        let base_size = bankroll * self.fixed_pct;
        let max_size = bankroll * self.max_pct;

        base_size.min(max_size).max(self.min_size)
    }

    /// Calculate position size with confidence adjustment
    ///
    /// Optionally scale size based on signal confidence
    pub fn calculate_with_confidence(&self, bankroll: Decimal, confidence: Decimal) -> Decimal {
        let base_size = self.calculate_size(bankroll);

        // Scale by confidence (0.5 to 1.0 range maps to 0.5x to 1.0x size)
        let confidence_factor = (confidence * dec!(0.5)) + dec!(0.5);
        (base_size * confidence_factor).max(self.min_size)
    }
}

impl Default for FixedSizer {
    fn default() -> Self {
        Self {
            fixed_pct: dec!(0.10), // 10% per trade
            max_pct: dec!(0.20),   // 20% max
            min_size: dec!(1),     // $1 min
        }
    }
}

impl PositionSizer for FixedSizer {
    fn calculate(&self, signal: &LagSignal, bankroll: Decimal) -> Decimal {
        // Use confidence-adjusted sizing for lag signals
        self.calculate_with_confidence(bankroll, signal.confidence)
    }

    fn mode_name(&self) -> &'static str {
        "fixed"
    }
}

/// Kelly criterion position sizing (adapted for lag signals)
///
/// Uses Kelly formula based on lag magnitude as edge estimate.
/// More aggressive than fixed sizing but theoretically optimal.
#[derive(Debug, Clone)]
pub struct KellySizer {
    /// Kelly fraction (e.g., 0.25 for quarter Kelly)
    pub fraction: Decimal,
    /// Maximum bet as percentage of bankroll
    pub max_pct: Decimal,
    /// Minimum trade size
    pub min_size: Decimal,
}

impl KellySizer {
    /// Create a new Kelly sizer
    pub fn new(fraction: Decimal, max_pct: Decimal) -> Self {
        Self {
            fraction,
            max_pct,
            min_size: dec!(1),
        }
    }

    /// Calculate Kelly size based on lag magnitude
    ///
    /// Assumes lag magnitude represents edge, and estimates win probability
    /// based on historical patterns (lag edges have ~80-95% win rate)
    pub fn calculate_from_lag(&self, lag_magnitude: Decimal, bankroll: Decimal) -> Decimal {
        // Estimate win probability based on lag size
        // Larger lag = higher confidence = higher win rate
        // Base: 80% + (lag * 50%), capped at 95%
        let win_prob = (dec!(0.80) + (lag_magnitude * dec!(0.5))).min(dec!(0.95));
        let lose_prob = Decimal::ONE - win_prob;

        // For binary outcome with 1:1 payout (simplified)
        // Kelly = (p * b - q) / b = p - q (when b = 1)
        let kelly = win_prob - lose_prob;

        if kelly <= dec!(0) {
            return dec!(0);
        }

        // Apply fractional Kelly
        let adjusted = kelly * self.fraction;

        // Calculate position
        let position = adjusted * bankroll;
        let max_size = bankroll * self.max_pct;

        position.min(max_size).max(self.min_size)
    }
}

impl Default for KellySizer {
    fn default() -> Self {
        Self {
            fraction: dec!(0.25), // Quarter Kelly
            max_pct: dec!(0.20),  // 20% max
            min_size: dec!(1),
        }
    }
}

impl PositionSizer for KellySizer {
    fn calculate(&self, signal: &LagSignal, bankroll: Decimal) -> Decimal {
        self.calculate_from_lag(signal.lag_magnitude, bankroll)
    }

    fn mode_name(&self) -> &'static str {
        "kelly"
    }
}

/// Create a position sizer based on configuration
pub fn create_sizer(config: &SizingConfig) -> Box<dyn PositionSizer> {
    match config.mode {
        crate::config::SizingMode::Fixed => Box::new(FixedSizer::from_config(config)),
        crate::config::SizingMode::Kelly => Box::new(KellySizer::new(dec!(0.25), config.max_pct)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lag::{OddsState, TradeSide};
    use crate::momentum::{MomentumDirection, MomentumSignal};

    fn create_test_signal(lag: Decimal, confidence: Decimal) -> LagSignal {
        let momentum = MomentumSignal::new(
            MomentumDirection::Up,
            dec!(0.01),
            dec!(95000),
            dec!(95950),
            dec!(0.0001),
            confidence,
        );
        let odds = OddsState::from_yes_price(dec!(0.50));

        LagSignal::new(
            TradeSide::Yes,
            lag,
            dec!(0.65),
            dec!(0.50),
            momentum,
            odds,
            180,
            720,
        )
    }

    #[test]
    fn test_fixed_sizer_new() {
        let sizer = FixedSizer::new(dec!(0.15), dec!(0.25));
        assert_eq!(sizer.fixed_pct, dec!(0.15));
        assert_eq!(sizer.max_pct, dec!(0.25));
    }

    #[test]
    fn test_fixed_sizer_default() {
        let sizer = FixedSizer::default();
        assert_eq!(sizer.fixed_pct, dec!(0.10));
        assert_eq!(sizer.max_pct, dec!(0.20));
    }

    #[test]
    fn test_fixed_sizer_calculate_size() {
        let sizer = FixedSizer::new(dec!(0.10), dec!(0.20));

        // $100 bankroll, 10% = $10
        assert_eq!(sizer.calculate_size(dec!(100)), dec!(10));

        // $1000 bankroll, 10% = $100
        assert_eq!(sizer.calculate_size(dec!(1000)), dec!(100));
    }

    #[test]
    fn test_fixed_sizer_respects_max() {
        let sizer = FixedSizer::new(dec!(0.30), dec!(0.20)); // 30% fixed, 20% max

        // $100 bankroll: 30% = $30, but max 20% = $20
        assert_eq!(sizer.calculate_size(dec!(100)), dec!(20));
    }

    #[test]
    fn test_fixed_sizer_respects_min() {
        let sizer = FixedSizer::new(dec!(0.10), dec!(0.20)).with_min_size(dec!(5));

        // $10 bankroll: 10% = $1, but min $5
        assert_eq!(sizer.calculate_size(dec!(10)), dec!(5));
    }

    #[test]
    fn test_fixed_sizer_with_confidence() {
        let sizer = FixedSizer::default();

        // Full confidence (1.0) -> factor = 1.0
        let high_conf = sizer.calculate_with_confidence(dec!(100), dec!(1.0));
        assert_eq!(high_conf, dec!(10)); // 10% of $100

        // Low confidence (0.5) -> factor = 0.75
        let low_conf = sizer.calculate_with_confidence(dec!(100), dec!(0.5));
        assert_eq!(low_conf, dec!(7.5)); // 75% of $10
    }

    #[test]
    fn test_fixed_sizer_position_sizer_trait() {
        let sizer = FixedSizer::default();
        let signal = create_test_signal(dec!(0.15), dec!(0.8));

        let size = sizer.calculate(&signal, dec!(100));
        // With 0.8 confidence: factor = 0.8 * 0.5 + 0.5 = 0.9
        // Base size = $10, adjusted = ~$9 (may vary slightly due to combined confidence)
        assert!(size > dec!(8) && size <= dec!(10));
    }

    #[test]
    fn test_kelly_sizer_new() {
        let sizer = KellySizer::new(dec!(0.25), dec!(0.15));
        assert_eq!(sizer.fraction, dec!(0.25));
        assert_eq!(sizer.max_pct, dec!(0.15));
    }

    #[test]
    fn test_kelly_sizer_calculate() {
        let sizer = KellySizer::default();

        // 15 cent lag -> win_prob = 0.80 + 0.15 * 0.5 = 0.875
        // Kelly = 0.875 - 0.125 = 0.75
        // Quarter Kelly = 0.1875
        // Size = 0.1875 * $100 = $18.75, capped at 20% = $20
        let size = sizer.calculate_from_lag(dec!(0.15), dec!(100));
        assert!(size > dec!(0));
        assert!(size <= dec!(20)); // Max 20%
    }

    #[test]
    fn test_kelly_sizer_small_lag() {
        let sizer = KellySizer::default();

        // Small lag = lower win probability
        let size = sizer.calculate_from_lag(dec!(0.05), dec!(100));
        assert!(size > dec!(0));
        assert!(size < dec!(20));
    }

    #[test]
    fn test_kelly_sizer_position_sizer_trait() {
        let sizer = KellySizer::default();
        let signal = create_test_signal(dec!(0.15), dec!(0.8));

        let size = sizer.calculate(&signal, dec!(100));
        assert!(size > dec!(0));
    }

    #[test]
    fn test_mode_names() {
        let fixed = FixedSizer::default();
        let kelly = KellySizer::default();

        assert_eq!(fixed.mode_name(), "fixed");
        assert_eq!(kelly.mode_name(), "kelly");
    }

    #[test]
    fn test_create_sizer_fixed() {
        let config = SizingConfig {
            mode: crate::config::SizingMode::Fixed,
            fixed_pct: dec!(0.15),
            max_pct: dec!(0.25),
        };

        let sizer = create_sizer(&config);
        assert_eq!(sizer.mode_name(), "fixed");
    }

    #[test]
    fn test_create_sizer_kelly() {
        let config = SizingConfig {
            mode: crate::config::SizingMode::Kelly,
            fixed_pct: dec!(0.15),
            max_pct: dec!(0.25),
        };

        let sizer = create_sizer(&config);
        assert_eq!(sizer.mode_name(), "kelly");
    }

    #[test]
    fn test_fixed_sizing_example_100_capital() {
        // Example from PRD: $100 capital, 10% sizing = $10 per trade
        let sizer = FixedSizer::new(dec!(0.10), dec!(0.20));
        let size = sizer.calculate_size(dec!(100));
        assert_eq!(size, dec!(10));
    }

    #[test]
    fn test_fixed_sizing_scales_with_bankroll() {
        let sizer = FixedSizer::new(dec!(0.10), dec!(0.20));

        // As bankroll grows, position size grows proportionally
        assert_eq!(sizer.calculate_size(dec!(100)), dec!(10));
        assert_eq!(sizer.calculate_size(dec!(500)), dec!(50));
        assert_eq!(sizer.calculate_size(dec!(1000)), dec!(100));
    }
}
