//! Position limits and drawdown controls

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// Position and risk limits
#[derive(Debug, Clone, Deserialize)]
pub struct PositionLimits {
    /// Maximum position as percentage of bankroll
    pub max_position_pct: Decimal,
    /// Maximum concurrent positions
    pub max_concurrent_positions: usize,
    /// Maximum daily loss percentage
    pub max_daily_loss_pct: Decimal,
    /// Maximum drawdown from peak percentage
    pub max_drawdown_pct: Decimal,
    /// Maximum total exposure percentage
    pub max_exposure_pct: Decimal,
}

impl Default for PositionLimits {
    fn default() -> Self {
        Self {
            max_position_pct: dec!(0.01),
            max_concurrent_positions: 3,
            max_daily_loss_pct: dec!(0.05),
            max_drawdown_pct: dec!(0.10),
            max_exposure_pct: dec!(0.10),
        }
    }
}

/// Reason for trading halt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HaltReason {
    /// Maximum daily loss reached
    MaxDailyLossReached(Decimal),
    /// Maximum drawdown from peak reached
    MaxDrawdownReached(Decimal),
    /// Maximum exposure reached
    MaxExposureReached(Decimal),
}

/// Monitors drawdown and triggers halts
pub struct DrawdownMonitor {
    /// Peak equity value
    pub peak_equity: Decimal,
    /// Current equity value
    pub current_equity: Decimal,
    /// Equity at start of day
    pub daily_start_equity: Decimal,
    /// Today's P&L
    pub daily_pnl: Decimal,
}

impl DrawdownMonitor {
    /// Create a new drawdown monitor
    pub fn new(initial_equity: Decimal) -> Self {
        Self {
            peak_equity: initial_equity,
            current_equity: initial_equity,
            daily_start_equity: initial_equity,
            daily_pnl: dec!(0),
        }
    }

    /// Update with new equity value
    pub fn update(&mut self, new_equity: Decimal) {
        self.current_equity = new_equity;
        if new_equity > self.peak_equity {
            self.peak_equity = new_equity;
        }
        self.daily_pnl = new_equity - self.daily_start_equity;
    }

    /// Get current drawdown from peak
    pub fn current_drawdown(&self) -> Decimal {
        if self.peak_equity == dec!(0) {
            return dec!(0);
        }
        (self.peak_equity - self.current_equity) / self.peak_equity
    }

    /// Get daily drawdown
    pub fn daily_drawdown(&self) -> Decimal {
        if self.daily_start_equity == dec!(0) {
            return dec!(0);
        }
        -self.daily_pnl / self.daily_start_equity
    }

    /// Check if trading should be halted
    pub fn should_halt(&self, limits: &PositionLimits) -> Option<HaltReason> {
        let daily_dd = self.daily_drawdown();
        if daily_dd > limits.max_daily_loss_pct {
            return Some(HaltReason::MaxDailyLossReached(daily_dd));
        }

        let drawdown = self.current_drawdown();
        if drawdown > limits.max_drawdown_pct {
            return Some(HaltReason::MaxDrawdownReached(drawdown));
        }

        None
    }

    /// Reset for new trading day
    pub fn reset_daily(&mut self) {
        self.daily_start_equity = self.current_equity;
        self.daily_pnl = dec!(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drawdown_monitor() {
        let mut monitor = DrawdownMonitor::new(dec!(1000));

        monitor.update(dec!(1100)); // New peak
        assert_eq!(monitor.peak_equity, dec!(1100));
        assert_eq!(monitor.current_drawdown(), dec!(0));

        monitor.update(dec!(990)); // Drawdown
        assert_eq!(monitor.current_drawdown(), dec!(0.10)); // 10%
    }

    #[test]
    fn test_halt_on_drawdown() {
        let mut monitor = DrawdownMonitor::new(dec!(1000));
        let limits = PositionLimits::default();

        // 15% down from start triggers daily loss first (5% limit)
        monitor.update(dec!(850));
        let halt = monitor.should_halt(&limits);
        assert!(matches!(halt, Some(HaltReason::MaxDailyLossReached(_))));

        // Reset daily to test drawdown from peak specifically
        monitor.reset_daily();
        // Now daily_start_equity = 850, peak_equity = 1000
        monitor.update(dec!(1000)); // Back to 1000
        monitor.update(dec!(850)); // 15% drawdown from peak, but daily_pnl = 0
        let halt = monitor.should_halt(&limits);
        // Daily is 0 (started at 850, now at 850), but drawdown from peak is 15%
        assert!(matches!(halt, Some(HaltReason::MaxDrawdownReached(_))));
    }
}
