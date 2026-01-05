//! Momentum detection types
//!
//! Types for representing detected price momentum relative to a strike price.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Direction of detected momentum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MomentumDirection {
    /// Price is above strike (bullish momentum)
    Up,
    /// Price is below strike (bearish momentum)
    Down,
}

/// A detected momentum signal
///
/// Represents a significant price move from the strike price that has been
/// confirmed over the configured window and confirmation period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MomentumSignal {
    /// Direction of the momentum (Up or Down)
    pub direction: MomentumDirection,

    /// Magnitude of the move as a decimal (e.g., 0.008 = 0.8%)
    pub move_pct: Decimal,

    /// The strike price (reference point, typically market open price)
    pub strike_price: Decimal,

    /// Current price when momentum was detected
    pub current_price: Decimal,

    /// Velocity of the move (change per second, for trend strength)
    pub velocity: Decimal,

    /// Timestamp when this momentum was detected
    pub detected_at: DateTime<Utc>,

    /// Confidence score (0.0 to 1.0) based on consistency of the move
    pub confidence: Decimal,
}

impl MomentumSignal {
    /// Create a new momentum signal
    pub fn new(
        direction: MomentumDirection,
        move_pct: Decimal,
        strike_price: Decimal,
        current_price: Decimal,
        velocity: Decimal,
        confidence: Decimal,
    ) -> Self {
        Self {
            direction,
            move_pct,
            strike_price,
            current_price,
            velocity,
            detected_at: Utc::now(),
            confidence,
        }
    }

    /// Check if momentum is bullish (price above strike)
    pub fn is_up(&self) -> bool {
        self.direction == MomentumDirection::Up
    }

    /// Check if momentum is bearish (price below strike)
    pub fn is_down(&self) -> bool {
        self.direction == MomentumDirection::Down
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_momentum_direction() {
        assert_eq!(MomentumDirection::Up, MomentumDirection::Up);
        assert_ne!(MomentumDirection::Up, MomentumDirection::Down);
    }

    #[test]
    fn test_momentum_signal_up() {
        let signal = MomentumSignal::new(
            MomentumDirection::Up,
            dec!(0.008),  // 0.8% move
            dec!(95000),  // strike
            dec!(95760),  // current
            dec!(0.0001), // velocity
            dec!(0.85),   // confidence
        );

        assert!(signal.is_up());
        assert!(!signal.is_down());
        assert_eq!(signal.move_pct, dec!(0.008));
    }

    #[test]
    fn test_momentum_signal_down() {
        let signal = MomentumSignal::new(
            MomentumDirection::Down,
            dec!(0.012),  // 1.2% move
            dec!(95000),  // strike
            dec!(93860),  // current
            dec!(0.0002), // velocity
            dec!(0.90),   // confidence
        );

        assert!(!signal.is_up());
        assert!(signal.is_down());
        assert_eq!(signal.move_pct, dec!(0.012));
    }
}
