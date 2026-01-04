//! Kelly criterion position sizing

use crate::signal::Signal;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Kelly criterion calculator for binary outcomes
pub struct KellyCalculator {
    /// Kelly fraction (e.g., 0.25 for quarter Kelly)
    pub fraction: Decimal,
    /// Maximum bet as percentage of bankroll
    pub max_bet_pct: Decimal,
}

impl KellyCalculator {
    /// Create a new Kelly calculator
    pub fn new(fraction: Decimal, max_bet_pct: Decimal) -> Self {
        Self {
            fraction,
            max_bet_pct,
        }
    }

    /// Calculate optimal position size
    ///
    /// For Polymarket binary markets:
    /// - Shares pay $1 if correct, $0 if wrong
    /// - Odds: b = (1 - market_price) / market_price
    /// - Kelly fraction: f* = (p*b - q) / b = (fair_value - market_price) / (1 - market_price)
    pub fn calculate(&self, signal: &Signal, bankroll: Decimal) -> Decimal {
        let edge = signal.fair_value - signal.market_price;

        if edge <= dec!(0) || signal.market_price >= dec!(1) {
            return dec!(0);
        }

        // Kelly fraction for binary bet
        let kelly_fraction = edge / (Decimal::ONE - signal.market_price);

        // Apply fractional Kelly
        let adjusted = kelly_fraction * self.fraction;

        // Calculate position size
        let position = adjusted * bankroll;

        // Apply hard cap
        let max_size = bankroll * self.max_bet_pct;
        position.min(max_size).max(dec!(0))
    }
}

impl Default for KellyCalculator {
    fn default() -> Self {
        Self::new(dec!(0.25), dec!(0.01))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::Market;
    use crate::signal::{Side, SignalReason};
    use chrono::{Duration, Utc};
    use rust_decimal_macros::dec;

    fn make_signal(fair_value: Decimal, market_price: Decimal) -> Signal {
        let now = Utc::now();
        Signal::new(
            Market {
                condition_id: "test".to_string(),
                yes_token_id: "yes".to_string(),
                no_token_id: "no".to_string(),
                open_price: dec!(100000),
                open_time: now,
                close_time: now + Duration::minutes(15),
            },
            Side::Yes,
            fair_value,
            market_price,
            fair_value - market_price,
            dec!(0.8),
            SignalReason::SpotDivergence,
        )
    }

    #[test]
    fn test_kelly_calculation() {
        let calc = KellyCalculator::new(dec!(0.25), dec!(0.01));
        let bankroll = dec!(1000);

        // 55% fair value, 50% market price = 5% edge
        let signal = make_signal(dec!(0.55), dec!(0.50));
        let size = calc.calculate(&signal, bankroll);

        // Kelly = 0.05 / 0.50 = 0.10
        // Quarter Kelly = 0.025
        // Size = 0.025 * 1000 = 25
        // But capped at 1% = 10
        assert_eq!(size, dec!(10));
    }

    #[test]
    fn test_kelly_no_edge() {
        let calc = KellyCalculator::default();
        let signal = make_signal(dec!(0.50), dec!(0.50));
        let size = calc.calculate(&signal, dec!(1000));
        assert_eq!(size, dec!(0));
    }
}
