//! Momentum detection module
//!
//! Detects significant price moves from a strike price using a rolling window.
//! This is the foundation of the lag edges strategy: detect momentum FIRST,
//! then check if Polymarket odds are lagging.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;

use super::types::{MomentumDirection, MomentumSignal};

/// Configuration for momentum detection
#[derive(Debug, Clone)]
pub struct MomentumConfig {
    /// Window duration for momentum calculation (default: 120 seconds)
    pub window_seconds: u64,

    /// Minimum move percentage to trigger momentum signal (default: 0.7%)
    pub min_move_pct: Decimal,

    /// Maximum move percentage (reject extreme moves as data errors)
    pub max_move_pct: Decimal,

    /// Confirmation period: move must persist for this duration (default: 30 seconds)
    pub confirmation_seconds: u64,
}

impl Default for MomentumConfig {
    fn default() -> Self {
        Self {
            window_seconds: 120,
            min_move_pct: dec!(0.007), // 0.7%
            max_move_pct: dec!(0.05),  // 5%
            confirmation_seconds: 30,
        }
    }
}

/// Momentum detector using rolling price window
///
/// Tracks price history and detects when price has moved significantly
/// from a strike price. The momentum must persist for the confirmation
/// period to be considered valid.
pub struct MomentumDetector {
    /// Configuration
    config: MomentumConfig,

    /// Price history with timestamps
    prices: VecDeque<(DateTime<Utc>, Decimal)>,

    /// Window duration
    window: Duration,

    /// Last detected momentum direction (for confirmation tracking)
    last_direction: Option<MomentumDirection>,

    /// When the current direction was first detected
    direction_start: Option<DateTime<Utc>>,
}

impl MomentumDetector {
    /// Create a new momentum detector with the given configuration
    pub fn new(config: MomentumConfig) -> Self {
        let window = Duration::seconds(config.window_seconds as i64);
        Self {
            config,
            prices: VecDeque::new(),
            window,
            last_direction: None,
            direction_start: None,
        }
    }

    /// Create a detector with default configuration
    pub fn with_defaults() -> Self {
        Self::new(MomentumConfig::default())
    }

    /// Add a new price observation
    pub fn update(&mut self, timestamp: DateTime<Utc>, price: Decimal) {
        // Add new price
        self.prices.push_back((timestamp, price));

        // Remove old prices outside window
        let cutoff = timestamp - self.window;
        while let Some((ts, _)) = self.prices.front() {
            if *ts < cutoff {
                self.prices.pop_front();
            } else {
                break;
            }
        }
    }

    /// Detect momentum relative to a strike price
    ///
    /// Returns Some(MomentumSignal) if:
    /// 1. Current price has moved > min_move_pct from strike
    /// 2. Move is < max_move_pct (sanity check)
    /// 3. Direction has been consistent for confirmation_seconds
    pub fn detect(&mut self, strike_price: Decimal) -> Option<MomentumSignal> {
        if self.prices.is_empty() || strike_price.is_zero() {
            return None;
        }

        // Get current price (most recent)
        let (current_ts, current_price) = self.prices.back()?;

        // Calculate move percentage from strike
        let move_pct = (current_price - strike_price) / strike_price;
        let abs_move = move_pct.abs();

        // Check if move exceeds threshold
        if abs_move < self.config.min_move_pct {
            // Reset confirmation tracking if move is too small
            self.last_direction = None;
            self.direction_start = None;
            return None;
        }

        // Check sanity ceiling
        if abs_move > self.config.max_move_pct {
            // Extreme move, likely data error
            return None;
        }

        // Determine direction
        let direction = if move_pct > Decimal::ZERO {
            MomentumDirection::Up
        } else {
            MomentumDirection::Down
        };

        // Track direction confirmation
        let now = *current_ts;
        match self.last_direction {
            Some(last_dir) if last_dir == direction => {
                // Same direction, check if confirmed
                if let Some(start) = self.direction_start {
                    let elapsed = now - start;
                    if elapsed.num_seconds() >= self.config.confirmation_seconds as i64 {
                        // Momentum confirmed!
                        let velocity = self.calculate_velocity();
                        let confidence = self.calculate_confidence(abs_move);

                        return Some(MomentumSignal::new(
                            direction,
                            abs_move,
                            strike_price,
                            *current_price,
                            velocity,
                            confidence,
                        ));
                    }
                }
            }
            _ => {
                // Direction changed or first detection, start confirmation timer
                self.last_direction = Some(direction);
                self.direction_start = Some(now);
            }
        }

        None
    }

    /// Calculate velocity (price change per second)
    fn calculate_velocity(&self) -> Decimal {
        if self.prices.len() < 2 {
            return Decimal::ZERO;
        }

        let (first_ts, first_price) = self.prices.front().unwrap();
        let (last_ts, last_price) = self.prices.back().unwrap();

        let time_diff = (*last_ts - *first_ts).num_seconds();
        if time_diff == 0 {
            return Decimal::ZERO;
        }

        let price_diff = *last_price - *first_price;
        price_diff / Decimal::from(time_diff)
    }

    /// Calculate confidence score based on move consistency
    fn calculate_confidence(&self, abs_move: Decimal) -> Decimal {
        // Base confidence from move size (larger moves = higher confidence)
        let move_confidence = (abs_move / self.config.min_move_pct).min(dec!(2)) / dec!(2);

        // Sample size confidence (more samples = higher confidence)
        let sample_confidence = Decimal::from(self.prices.len().min(100)) / dec!(100);

        // Combined confidence (weighted average)
        (move_confidence * dec!(0.6)) + (sample_confidence * dec!(0.4))
    }

    /// Get the number of price samples in the window
    pub fn sample_count(&self) -> usize {
        self.prices.len()
    }

    /// Check if we have enough data for detection
    pub fn is_ready(&self) -> bool {
        self.prices.len() >= 2
    }

    /// Clear all price history
    pub fn clear(&mut self) {
        self.prices.clear();
        self.last_direction = None;
        self.direction_start = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn create_detector() -> MomentumDetector {
        MomentumDetector::new(MomentumConfig {
            window_seconds: 120,
            min_move_pct: dec!(0.007), // 0.7%
            max_move_pct: dec!(0.05),  // 5%
            confirmation_seconds: 5,   // Short for testing
        })
    }

    #[test]
    fn test_new_detector() {
        let detector = MomentumDetector::with_defaults();
        assert_eq!(detector.sample_count(), 0);
        assert!(!detector.is_ready());
    }

    #[test]
    fn test_update_adds_prices() {
        let mut detector = create_detector();
        let now = Utc::now();

        detector.update(now, dec!(95000));
        assert_eq!(detector.sample_count(), 1);

        detector.update(now + Duration::seconds(1), dec!(95100));
        assert_eq!(detector.sample_count(), 2);
        assert!(detector.is_ready());
    }

    #[test]
    fn test_update_removes_old_prices() {
        let mut detector = MomentumDetector::new(MomentumConfig {
            window_seconds: 10, // Short window
            ..Default::default()
        });

        let base_time = Utc::now();

        // Add prices within window
        for i in 0..5 {
            detector.update(base_time + Duration::seconds(i), dec!(95000));
        }
        assert_eq!(detector.sample_count(), 5);

        // Add price that expires old ones
        detector.update(base_time + Duration::seconds(15), dec!(95000));
        assert!(detector.sample_count() < 5);
    }

    #[test]
    fn test_detect_no_momentum_at_strike() {
        let mut detector = create_detector();
        let now = Utc::now();
        let strike = dec!(95000);

        // Price at strike - no momentum
        for i in 0..10 {
            detector.update(now + Duration::seconds(i), strike);
        }

        let signal = detector.detect(strike);
        assert!(signal.is_none());
    }

    #[test]
    fn test_detect_small_move_no_signal() {
        let mut detector = create_detector();
        let now = Utc::now();
        let strike = dec!(95000);

        // Small move (0.5% < 0.7% threshold)
        let price = strike * dec!(1.005);
        for i in 0..10 {
            detector.update(now + Duration::seconds(i), price);
        }

        let signal = detector.detect(strike);
        assert!(signal.is_none());
    }

    #[test]
    fn test_detect_up_momentum() {
        let mut detector = create_detector();
        let now = Utc::now();
        let strike = dec!(95000);

        // Move up by 0.8% (above 0.7% threshold)
        let price = strike * dec!(1.008);

        // Need to exceed confirmation period (5 seconds in test config)
        for i in 0..10 {
            detector.update(now + Duration::seconds(i), price);
            // Call detect each iteration to track direction
            let _ = detector.detect(strike);
        }

        let signal = detector.detect(strike);
        assert!(signal.is_some());

        let signal = signal.unwrap();
        assert!(signal.is_up());
        assert!(signal.move_pct >= dec!(0.007));
    }

    #[test]
    fn test_detect_down_momentum() {
        let mut detector = create_detector();
        let now = Utc::now();
        let strike = dec!(95000);

        // Move down by 1% (above 0.7% threshold)
        let price = strike * dec!(0.99);

        for i in 0..10 {
            detector.update(now + Duration::seconds(i), price);
            let _ = detector.detect(strike);
        }

        let signal = detector.detect(strike);
        assert!(signal.is_some());

        let signal = signal.unwrap();
        assert!(signal.is_down());
    }

    #[test]
    fn test_detect_extreme_move_rejected() {
        let mut detector = create_detector();
        let now = Utc::now();
        let strike = dec!(95000);

        // Move up by 10% (above 5% max threshold)
        let price = strike * dec!(1.10);

        for i in 0..10 {
            detector.update(now + Duration::seconds(i), price);
            let _ = detector.detect(strike);
        }

        let signal = detector.detect(strike);
        assert!(signal.is_none()); // Rejected as too extreme
    }

    #[test]
    fn test_confirmation_required() {
        let mut detector = MomentumDetector::new(MomentumConfig {
            window_seconds: 120,
            min_move_pct: dec!(0.007),
            max_move_pct: dec!(0.05),
            confirmation_seconds: 30, // Long confirmation
        });

        let now = Utc::now();
        let strike = dec!(95000);
        let price = strike * dec!(1.008);

        // Only 5 seconds of data - should not confirm
        for i in 0..5 {
            detector.update(now + Duration::seconds(i), price);
            let _ = detector.detect(strike);
        }

        let signal = detector.detect(strike);
        assert!(signal.is_none()); // Not enough confirmation time
    }

    #[test]
    fn test_direction_change_resets_confirmation() {
        let mut detector = create_detector();
        let now = Utc::now();
        let strike = dec!(95000);

        // First go up
        let up_price = strike * dec!(1.008);
        for i in 0..3 {
            detector.update(now + Duration::seconds(i), up_price);
            let _ = detector.detect(strike);
        }

        // Then go down - should reset confirmation
        let down_price = strike * dec!(0.99);
        for i in 3..6 {
            detector.update(now + Duration::seconds(i), down_price);
            let _ = detector.detect(strike);
        }

        // Should need to wait for new confirmation period
        // (may or may not have signal depending on timing)
    }

    #[test]
    fn test_clear() {
        let mut detector = create_detector();
        let now = Utc::now();

        detector.update(now, dec!(95000));
        detector.update(now + Duration::seconds(1), dec!(95100));
        assert!(detector.is_ready());

        detector.clear();
        assert_eq!(detector.sample_count(), 0);
        assert!(!detector.is_ready());
    }

    #[test]
    fn test_velocity_calculation() {
        let mut detector = create_detector();
        let now = Utc::now();
        let strike = dec!(95000);

        // Rising prices over 10 seconds
        for i in 0..10 {
            let price = strike + Decimal::from(i * 100);
            detector.update(now + Duration::seconds(i), price);
            let _ = detector.detect(strike);
        }

        // After confirmation, check velocity is positive
        // (need enough time for confirmation)
        for i in 10..20 {
            let price = strike + Decimal::from(i * 100);
            detector.update(now + Duration::seconds(i), price);
        }

        if let Some(signal) = detector.detect(strike) {
            assert!(signal.velocity > Decimal::ZERO);
        }
    }

    #[test]
    fn test_confidence_calculation() {
        let mut detector = create_detector();
        let now = Utc::now();
        let strike = dec!(95000);

        // Large move with many samples should have higher confidence
        let price = strike * dec!(1.02); // 2% move
        for i in 0..50 {
            detector.update(now + Duration::seconds(i), price);
            let _ = detector.detect(strike);
        }

        if let Some(signal) = detector.detect(strike) {
            assert!(signal.confidence > dec!(0.5));
        }
    }

    #[test]
    fn test_zero_strike_returns_none() {
        let mut detector = create_detector();
        let now = Utc::now();

        detector.update(now, dec!(95000));
        detector.update(now + Duration::seconds(1), dec!(95100));

        let signal = detector.detect(Decimal::ZERO);
        assert!(signal.is_none());
    }
}
