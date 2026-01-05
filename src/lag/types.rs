//! Lag detection types
//!
//! Types for representing detected lag between spot price momentum
//! and Polymarket odds.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::momentum::MomentumSignal;

/// Trading side for the signal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeSide {
    /// Buy YES tokens (bullish)
    Yes,
    /// Buy NO tokens (bearish)
    No,
}

impl TradeSide {
    /// Get the opposite side
    pub fn opposite(&self) -> Self {
        match self {
            TradeSide::Yes => TradeSide::No,
            TradeSide::No => TradeSide::Yes,
        }
    }
}

/// Current state of Polymarket odds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OddsState {
    /// Current YES price (0.0 to 1.0)
    pub yes_price: Decimal,
    /// Current NO price (0.0 to 1.0)
    pub no_price: Decimal,
    /// Spread between best bid and ask
    pub spread: Option<Decimal>,
    /// Timestamp of this odds snapshot
    pub timestamp: DateTime<Utc>,
}

impl OddsState {
    /// Create a new odds state
    pub fn new(yes_price: Decimal, no_price: Decimal) -> Self {
        Self {
            yes_price,
            no_price,
            spread: None,
            timestamp: Utc::now(),
        }
    }

    /// Create from YES price only (NO = 1 - YES)
    pub fn from_yes_price(yes_price: Decimal) -> Self {
        Self {
            yes_price,
            no_price: Decimal::ONE - yes_price,
            spread: None,
            timestamp: Utc::now(),
        }
    }

    /// Check if odds are in the neutral zone (40-60 cents)
    pub fn is_neutral(&self, min_yes: Decimal, max_yes: Decimal) -> bool {
        self.yes_price >= min_yes && self.yes_price <= max_yes
    }

    /// Check if odds favor YES (above neutral)
    pub fn favors_yes(&self, threshold: Decimal) -> bool {
        self.yes_price > threshold
    }

    /// Check if odds favor NO (below neutral)
    pub fn favors_no(&self, threshold: Decimal) -> bool {
        self.yes_price < threshold
    }
}

/// A detected lag signal
///
/// Represents a situation where Polymarket odds are lagging behind
/// confirmed spot price momentum. This is the core trading signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LagSignal {
    /// Which side to trade (YES or NO)
    pub side: TradeSide,

    /// Magnitude of the lag in cents (e.g., 0.15 = 15 cents)
    pub lag_magnitude: Decimal,

    /// Expected price based on momentum
    pub expected_price: Decimal,

    /// Actual current price
    pub actual_price: Decimal,

    /// The momentum that triggered this signal
    pub momentum: MomentumSignal,

    /// Current odds state
    pub odds: OddsState,

    /// Confidence score (0.0 to 1.0)
    pub confidence: Decimal,

    /// Timestamp when lag was detected
    pub detected_at: DateTime<Utc>,

    /// Seconds since market opened
    pub seconds_since_open: i64,

    /// Seconds until market closes
    pub seconds_until_close: i64,
}

impl LagSignal {
    /// Create a new lag signal
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        side: TradeSide,
        lag_magnitude: Decimal,
        expected_price: Decimal,
        actual_price: Decimal,
        momentum: MomentumSignal,
        odds: OddsState,
        seconds_since_open: i64,
        seconds_until_close: i64,
    ) -> Self {
        // Confidence based on lag size and momentum confidence
        let lag_confidence = (lag_magnitude / Decimal::new(20, 2)).min(Decimal::ONE); // 20 cents = max
        let combined_confidence = (lag_confidence + momentum.confidence) / Decimal::TWO;

        Self {
            side,
            lag_magnitude,
            expected_price,
            actual_price,
            momentum,
            odds,
            confidence: combined_confidence,
            detected_at: Utc::now(),
            seconds_since_open,
            seconds_until_close,
        }
    }

    /// Check if this is a YES signal
    pub fn is_yes(&self) -> bool {
        self.side == TradeSide::Yes
    }

    /// Check if this is a NO signal
    pub fn is_no(&self) -> bool {
        self.side == TradeSide::No
    }

    /// Get the entry price for this signal
    pub fn entry_price(&self) -> Decimal {
        self.actual_price
    }

    /// Check if signal is in the prime trading window (5-12 min after open)
    pub fn is_prime_window(&self) -> bool {
        self.seconds_since_open >= 300 && self.seconds_since_open <= 720
    }
}

/// Reason why no lag signal was generated
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoLagReason {
    /// No momentum detected
    NoMomentum,
    /// Odds already reflect the momentum (no lag)
    OddsAlreadyMoved,
    /// Lag is below minimum threshold
    LagTooSmall,
    /// Too early in the market window
    TooEarlyInWindow,
    /// Too close to market close
    TooCloseToClose,
    /// Missing order book data
    NoOrderBookData,
    /// Market not active
    MarketNotActive,
}

impl std::fmt::Display for NoLagReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoLagReason::NoMomentum => write!(f, "No momentum detected"),
            NoLagReason::OddsAlreadyMoved => write!(f, "Odds already reflect momentum"),
            NoLagReason::LagTooSmall => write!(f, "Lag below minimum threshold"),
            NoLagReason::TooEarlyInWindow => write!(f, "Too early in market window"),
            NoLagReason::TooCloseToClose => write!(f, "Too close to market close"),
            NoLagReason::NoOrderBookData => write!(f, "No order book data available"),
            NoLagReason::MarketNotActive => write!(f, "Market not active"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::momentum::MomentumDirection;
    use rust_decimal_macros::dec;

    #[test]
    fn test_trade_side() {
        assert_eq!(TradeSide::Yes.opposite(), TradeSide::No);
        assert_eq!(TradeSide::No.opposite(), TradeSide::Yes);
    }

    #[test]
    fn test_odds_state_new() {
        let odds = OddsState::new(dec!(0.55), dec!(0.45));
        assert_eq!(odds.yes_price, dec!(0.55));
        assert_eq!(odds.no_price, dec!(0.45));
    }

    #[test]
    fn test_odds_state_from_yes_price() {
        let odds = OddsState::from_yes_price(dec!(0.60));
        assert_eq!(odds.yes_price, dec!(0.60));
        assert_eq!(odds.no_price, dec!(0.40));
    }

    #[test]
    fn test_odds_state_is_neutral() {
        let odds = OddsState::from_yes_price(dec!(0.52));
        assert!(odds.is_neutral(dec!(0.40), dec!(0.60)));

        let high_odds = OddsState::from_yes_price(dec!(0.70));
        assert!(!high_odds.is_neutral(dec!(0.40), dec!(0.60)));

        let low_odds = OddsState::from_yes_price(dec!(0.30));
        assert!(!low_odds.is_neutral(dec!(0.40), dec!(0.60)));
    }

    #[test]
    fn test_odds_favors() {
        let odds = OddsState::from_yes_price(dec!(0.65));
        assert!(odds.favors_yes(dec!(0.60)));
        assert!(!odds.favors_no(dec!(0.40)));

        let low_odds = OddsState::from_yes_price(dec!(0.35));
        assert!(low_odds.favors_no(dec!(0.40)));
        assert!(!low_odds.favors_yes(dec!(0.60)));
    }

    #[test]
    fn test_lag_signal_new() {
        let momentum = MomentumSignal::new(
            MomentumDirection::Up,
            dec!(0.01),
            dec!(95000),
            dec!(95950),
            dec!(0.0001),
            dec!(0.8),
        );
        let odds = OddsState::from_yes_price(dec!(0.52));

        let signal = LagSignal::new(
            TradeSide::Yes,
            dec!(0.15),
            dec!(0.67),
            dec!(0.52),
            momentum,
            odds,
            180, // 3 min after open
            720, // 12 min until close
        );

        assert!(signal.is_yes());
        assert!(!signal.is_no());
        assert_eq!(signal.lag_magnitude, dec!(0.15));
        assert_eq!(signal.entry_price(), dec!(0.52));
    }

    #[test]
    fn test_lag_signal_prime_window() {
        let momentum = MomentumSignal::new(
            MomentumDirection::Up,
            dec!(0.01),
            dec!(95000),
            dec!(95950),
            dec!(0.0001),
            dec!(0.8),
        );
        let odds = OddsState::from_yes_price(dec!(0.52));

        // In prime window (5-12 min = 300-720 seconds)
        let prime_signal = LagSignal::new(
            TradeSide::Yes,
            dec!(0.15),
            dec!(0.67),
            dec!(0.52),
            momentum.clone(),
            odds.clone(),
            400,
            500,
        );
        assert!(prime_signal.is_prime_window());

        // Too early (2 min)
        let early_signal = LagSignal::new(
            TradeSide::Yes,
            dec!(0.15),
            dec!(0.67),
            dec!(0.52),
            momentum.clone(),
            odds.clone(),
            120,
            780,
        );
        assert!(!early_signal.is_prime_window());

        // Too late (13 min)
        let late_signal = LagSignal::new(
            TradeSide::Yes,
            dec!(0.15),
            dec!(0.67),
            dec!(0.52),
            momentum,
            odds,
            780,
            120,
        );
        assert!(!late_signal.is_prime_window());
    }

    #[test]
    fn test_no_lag_reason_display() {
        assert_eq!(NoLagReason::NoMomentum.to_string(), "No momentum detected");
        assert_eq!(
            NoLagReason::OddsAlreadyMoved.to_string(),
            "Odds already reflect momentum"
        );
        assert_eq!(
            NoLagReason::LagTooSmall.to_string(),
            "Lag below minimum threshold"
        );
    }
}
