//! Momentum-first signal detection
//!
//! Implements the lag edges strategy: detect spot momentum FIRST,
//! then check if Polymarket odds are lagging behind.

use super::{Side, Signal, SignalReason};
use crate::config::LagConfig;
use crate::lag::{LagDetector, LagDetectorConfig, LagSignal, NoLagReason, OddsState, TradeSide};
use crate::market::Market;
use crate::momentum::{MomentumConfig, MomentumDetector};
use crate::orderbook::OrderBook;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Momentum-first signal detector for the lag edges strategy
///
/// This detector:
/// 1. Tracks spot price momentum from Binance
/// 2. Detects confirmed momentum (>0.7% move over 2 min)
/// 3. Checks if Polymarket odds are still in neutral zone
/// 4. Generates trading signal if lag exists
pub struct MomentumSignalDetector {
    momentum_detector: MomentumDetector,
    lag_detector: LagDetector,
    fee_rate: Decimal,
    slippage_estimate: Decimal,
}

impl MomentumSignalDetector {
    /// Create a new momentum signal detector with default configs
    pub fn new(fee_rate: Decimal, slippage_estimate: Decimal) -> Self {
        Self {
            momentum_detector: MomentumDetector::with_defaults(),
            lag_detector: LagDetector::new(),
            fee_rate,
            slippage_estimate,
        }
    }

    /// Create with custom momentum and lag configurations
    pub fn with_configs(
        momentum_config: MomentumConfig,
        lag_config: LagDetectorConfig,
        fee_rate: Decimal,
        slippage_estimate: Decimal,
    ) -> Self {
        Self {
            momentum_detector: MomentumDetector::new(momentum_config),
            lag_detector: LagDetector::with_config(lag_config),
            fee_rate,
            slippage_estimate,
        }
    }

    /// Create from application configs
    pub fn from_configs(
        momentum_config: &crate::config::MomentumConfig,
        lag_config: &LagConfig,
        fee_rate: Decimal,
        slippage_estimate: Decimal,
    ) -> Self {
        let momentum = MomentumConfig {
            window_seconds: momentum_config.window_seconds,
            min_move_pct: momentum_config.min_move_pct,
            max_move_pct: momentum_config.max_move_pct,
            confirmation_seconds: momentum_config.confirmation_seconds,
        };

        Self {
            momentum_detector: MomentumDetector::new(momentum),
            lag_detector: LagDetector::from_lag_config(lag_config),
            fee_rate,
            slippage_estimate,
        }
    }

    /// Update with a new price observation
    ///
    /// Call this on each price tick from Binance
    pub fn update_price(&mut self, timestamp: DateTime<Utc>, price: Decimal) {
        self.momentum_detector.update(timestamp, price);
    }

    /// Detect if there's a trading opportunity
    ///
    /// Returns Some(Signal) if:
    /// 1. Momentum is confirmed (price moved >threshold from strike)
    /// 2. Polymarket odds are still lagging (in neutral zone)
    /// 3. We're in the valid trading window (not too early/late)
    pub fn detect(&mut self, market: &Market, orderbook: &OrderBook) -> Option<Signal> {
        // Step 1: Check for confirmed momentum
        let momentum = self.momentum_detector.detect(market.open_price)?;

        // Step 2: Get current odds from order book
        let odds = self.get_odds_state(orderbook)?;

        // Step 3: Check for lag
        let lag_signal = match self.lag_detector.detect(&momentum, &odds, market) {
            Ok(Some(signal)) => signal,
            Ok(None) => return None,
            Err(reason) => {
                tracing::trace!(
                    reason = %reason,
                    "No lag signal"
                );
                return None;
            }
        };

        // Step 4: Convert to Signal
        Some(self.lag_signal_to_signal(lag_signal, market))
    }

    /// Detect with explicit timestamp (for testing/backtesting)
    pub fn detect_at(
        &mut self,
        market: &Market,
        orderbook: &OrderBook,
        now: DateTime<Utc>,
    ) -> Option<Signal> {
        let momentum = self.momentum_detector.detect(market.open_price)?;
        let odds = self.get_odds_state(orderbook)?;

        let lag_signal = match self.lag_detector.detect_at(&momentum, &odds, market, now) {
            Ok(Some(signal)) => signal,
            Ok(None) => return None,
            Err(_) => return None,
        };

        Some(self.lag_signal_to_signal(lag_signal, market))
    }

    /// Check if momentum detection is ready (has enough data)
    pub fn is_ready(&self) -> bool {
        self.momentum_detector.is_ready()
    }

    /// Get current sample count
    pub fn sample_count(&self) -> usize {
        self.momentum_detector.sample_count()
    }

    /// Clear all price history
    pub fn clear(&mut self) {
        self.momentum_detector.clear();
    }

    /// Get odds state from order book
    fn get_odds_state(&self, orderbook: &OrderBook) -> Option<OddsState> {
        let yes_price = orderbook.best_ask()?;
        let spread = orderbook.spread();

        Some(OddsState {
            yes_price,
            no_price: Decimal::ONE - yes_price,
            spread,
            timestamp: orderbook.updated_at,
        })
    }

    /// Convert LagSignal to Signal
    fn lag_signal_to_signal(&self, lag: LagSignal, market: &Market) -> Signal {
        let side = match lag.side {
            TradeSide::Yes => Side::Yes,
            TradeSide::No => Side::No,
        };

        // Fair value is the expected price based on momentum
        let fair_value = lag.expected_price;
        let market_price = lag.actual_price;
        let raw_edge = lag.lag_magnitude;

        // Adjust for costs
        let total_costs = self.fee_rate + self.slippage_estimate;
        let adjusted_edge = (raw_edge - total_costs).max(dec!(0));

        // Determine reason based on momentum characteristics
        let reason = if lag.seconds_since_open < 120 {
            SignalReason::PostResetLag
        } else {
            SignalReason::SpotDivergence
        };

        Signal::new(
            market.clone(),
            side,
            fair_value,
            market_price,
            adjusted_edge,
            lag.confidence,
            reason,
        )
    }

    /// Get the momentum detector (for inspection)
    pub fn momentum_detector(&self) -> &MomentumDetector {
        &self.momentum_detector
    }

    /// Get the momentum detector mutably (for detection)
    pub fn momentum_detector_mut(&mut self) -> &mut MomentumDetector {
        &mut self.momentum_detector
    }

    /// Get the lag detector (for inspection)
    pub fn lag_detector(&self) -> &LagDetector {
        &self.lag_detector
    }
}

/// Result type for detection attempts
#[derive(Debug, Clone)]
pub enum DetectionResult {
    /// Signal generated (boxed to reduce enum size)
    Signal(Box<Signal>),
    /// No momentum detected
    NoMomentum,
    /// Momentum detected but no lag
    NoLag(NoLagReason),
    /// Missing order book data
    NoOrderBook,
    /// Not enough price samples yet
    NotReady,
}

impl MomentumSignalDetector {
    /// Detect with detailed result information
    pub fn detect_with_reason(
        &mut self,
        market: &Market,
        orderbook: &OrderBook,
    ) -> DetectionResult {
        if !self.is_ready() {
            return DetectionResult::NotReady;
        }

        // Check for momentum
        let momentum = match self.momentum_detector.detect(market.open_price) {
            Some(m) => m,
            None => return DetectionResult::NoMomentum,
        };

        // Get odds
        let odds = match self.get_odds_state(orderbook) {
            Some(o) => o,
            None => return DetectionResult::NoOrderBook,
        };

        // Check for lag
        match self.lag_detector.detect(&momentum, &odds, market) {
            Ok(Some(lag_signal)) => {
                let signal = self.lag_signal_to_signal(lag_signal, market);
                DetectionResult::Signal(Box::new(signal))
            }
            Ok(None) => DetectionResult::NoLag(NoLagReason::LagTooSmall),
            Err(reason) => DetectionResult::NoLag(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::momentum::MomentumSignal;
    use crate::orderbook::PriceLevel;
    use chrono::Duration;

    fn create_test_market() -> Market {
        let now = Utc::now();
        Market {
            condition_id: "test".to_string(),
            yes_token_id: "yes".to_string(),
            no_token_id: "no".to_string(),
            open_price: dec!(95000),
            open_time: now - Duration::minutes(5),
            close_time: now + Duration::minutes(10),
        }
    }

    fn create_test_orderbook(yes_ask: Decimal) -> OrderBook {
        OrderBook {
            token_id: "yes".to_string(),
            bids: vec![PriceLevel {
                price: yes_ask - dec!(0.02),
                size: dec!(100),
            }],
            asks: vec![PriceLevel {
                price: yes_ask,
                size: dec!(100),
            }],
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_detector_creation() {
        let detector = MomentumSignalDetector::new(dec!(0.005), dec!(0.002));
        assert!(!detector.is_ready());
        assert_eq!(detector.sample_count(), 0);
    }

    #[test]
    fn test_update_price() {
        let mut detector = MomentumSignalDetector::new(dec!(0.005), dec!(0.002));
        let now = Utc::now();

        detector.update_price(now, dec!(95000));
        assert_eq!(detector.sample_count(), 1);

        detector.update_price(now + Duration::seconds(1), dec!(95100));
        assert_eq!(detector.sample_count(), 2);
        assert!(detector.is_ready());
    }

    #[test]
    fn test_detect_no_momentum() {
        let mut detector = MomentumSignalDetector::new(dec!(0.005), dec!(0.002));
        let market = create_test_market();
        let orderbook = create_test_orderbook(dec!(0.50));
        let now = Utc::now();

        // Add prices at strike (no movement)
        for i in 0..10 {
            detector.update_price(now + Duration::seconds(i), dec!(95000));
        }

        let signal = detector.detect(&market, &orderbook);
        assert!(signal.is_none());
    }

    #[test]
    fn test_detect_with_momentum_and_lag() {
        let mut detector = MomentumSignalDetector::with_configs(
            MomentumConfig {
                window_seconds: 120,
                min_move_pct: dec!(0.007),
                max_move_pct: dec!(0.05),
                confirmation_seconds: 5, // Short for testing
            },
            LagDetectorConfig {
                min_lag_cents: dec!(0.05), // Lower threshold for testing
                max_yes_for_up: dec!(0.60),
                min_yes_for_down: dec!(0.40),
                min_seconds_after_open: 60,
                max_seconds_before_close: 120,
                price_sensitivity: dec!(10),
            },
            dec!(0.005),
            dec!(0.002),
        );

        let market = create_test_market();
        // Odds still neutral at 0.48 despite momentum
        let orderbook = create_test_orderbook(dec!(0.48));
        let base_time = market.open_time + Duration::minutes(2);

        // Simulate 1% price increase (above 0.7% threshold)
        // Use consistent high price to allow momentum confirmation
        let high_price = market.open_price * dec!(1.01); // 1% up

        // Need to call detect_at on each tick to build up confirmation
        // (momentum detector requires multiple detect() calls to confirm direction)
        for i in 0..10 {
            let detect_time = base_time + Duration::seconds(i);
            detector.update_price(detect_time, high_price);
            // Call detect to track confirmation state
            let _ = detector.detect_at(&market, &orderbook, detect_time);
        }

        // Now detect at a time after confirmation period
        let detect_time = base_time + Duration::seconds(10);
        let signal = detector.detect_at(&market, &orderbook, detect_time);

        // Should generate a YES signal since price is up but odds are neutral
        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert_eq!(signal.side, Side::Yes);
        assert!(signal.adjusted_edge > dec!(0));
    }

    #[test]
    fn test_detect_no_lag_odds_moved() {
        let mut detector = MomentumSignalDetector::with_configs(
            MomentumConfig {
                window_seconds: 120,
                min_move_pct: dec!(0.007),
                max_move_pct: dec!(0.05),
                confirmation_seconds: 5,
            },
            LagDetectorConfig::default(),
            dec!(0.005),
            dec!(0.002),
        );

        let market = create_test_market();
        // Odds already moved up to 0.70 - no lag
        let orderbook = create_test_orderbook(dec!(0.70));
        let base_time = market.open_time + Duration::minutes(2);

        // Add momentum
        for i in 0..10 {
            let price = dec!(95000) + Decimal::from(i * 100);
            detector.update_price(base_time + Duration::seconds(i), price);
        }

        let signal = detector.detect(&market, &orderbook);
        assert!(signal.is_none()); // No signal - odds already reflect momentum
    }

    #[test]
    fn test_clear() {
        let mut detector = MomentumSignalDetector::new(dec!(0.005), dec!(0.002));
        let now = Utc::now();

        detector.update_price(now, dec!(95000));
        detector.update_price(now + Duration::seconds(1), dec!(95100));
        assert!(detector.is_ready());

        detector.clear();
        assert!(!detector.is_ready());
        assert_eq!(detector.sample_count(), 0);
    }

    #[test]
    fn test_detection_result_variants() {
        let mut detector = MomentumSignalDetector::new(dec!(0.005), dec!(0.002));
        let market = create_test_market();
        let orderbook = create_test_orderbook(dec!(0.50));

        // Not ready yet
        let result = detector.detect_with_reason(&market, &orderbook);
        assert!(matches!(result, DetectionResult::NotReady));

        // Add some prices
        let now = Utc::now();
        for i in 0..5 {
            detector.update_price(now + Duration::seconds(i), dec!(95000));
        }

        // Now should be ready but no momentum
        let result = detector.detect_with_reason(&market, &orderbook);
        assert!(matches!(result, DetectionResult::NoMomentum));
    }

    #[test]
    fn test_get_odds_state() {
        let detector = MomentumSignalDetector::new(dec!(0.005), dec!(0.002));
        let orderbook = create_test_orderbook(dec!(0.55));

        let odds = detector.get_odds_state(&orderbook).unwrap();
        assert_eq!(odds.yes_price, dec!(0.55));
        assert_eq!(odds.no_price, dec!(0.45));
        assert!(odds.spread.is_some());
    }

    #[test]
    fn test_lag_signal_conversion() {
        let detector = MomentumSignalDetector::new(dec!(0.005), dec!(0.002));
        let market = create_test_market();

        let momentum = MomentumSignal::new(
            crate::momentum::MomentumDirection::Up,
            dec!(0.01),
            dec!(95000),
            dec!(95950),
            dec!(0.0001),
            dec!(0.8),
        );

        let lag = LagSignal::new(
            TradeSide::Yes,
            dec!(0.12),
            dec!(0.60),
            dec!(0.48),
            momentum,
            OddsState::from_yes_price(dec!(0.48)),
            180,
            720,
        );

        let signal = detector.lag_signal_to_signal(lag, &market);
        assert_eq!(signal.side, Side::Yes);
        assert_eq!(signal.fair_value, dec!(0.60));
        assert_eq!(signal.market_price, dec!(0.48));
        // Raw edge 0.12 - costs (0.005 + 0.002) = 0.113
        assert!(signal.adjusted_edge > dec!(0.11));
    }
}
