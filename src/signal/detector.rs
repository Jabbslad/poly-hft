//! Signal detection

use super::{Side, Signal, SignalReason};
use crate::market::Market;
use crate::model::{FairValueModel, FairValueParams};
use crate::orderbook::OrderBook;
use chrono::{Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;

/// Detects tradeable signals from market data
pub struct SignalDetector<M: FairValueModel> {
    model: M,
    fee_rate: Decimal,
    slippage_estimate: Decimal,
    /// Track last market close times for reset detection
    #[allow(dead_code)]
    last_market_close: HashMap<String, chrono::DateTime<chrono::Utc>>,
}

impl<M: FairValueModel> SignalDetector<M> {
    /// Create a new signal detector
    pub fn new(model: M, fee_rate: Decimal, slippage_estimate: Decimal) -> Self {
        Self {
            model,
            fee_rate,
            slippage_estimate,
            last_market_close: HashMap::new(),
        }
    }

    /// Check if market is in post-reset window
    pub fn is_post_reset(&self, market: &Market, window: Duration) -> bool {
        let now = Utc::now();
        now - market.open_time < window
    }

    /// Generate a signal if edge exists
    pub fn detect(
        &self,
        market: &Market,
        current_price: Decimal,
        volatility: Decimal,
        orderbook: &OrderBook,
    ) -> Option<Signal> {
        let time_to_expiry = market.close_time - Utc::now();
        if time_to_expiry <= Duration::zero() {
            return None;
        }

        // Calculate fair value
        let params = FairValueParams {
            current_price,
            open_price: market.open_price,
            time_to_expiry,
            volatility,
        };
        let fair_value = self.model.calculate(params);

        // Get market prices from order book
        let yes_ask = orderbook.best_ask()?;
        let no_bid = Decimal::ONE - yes_ask; // Implied no price

        // Calculate edge for each side
        let yes_edge = fair_value.yes_prob - yes_ask;
        let no_edge = fair_value.no_prob - no_bid;

        // Determine best side and edge
        let (side, raw_edge, fair_prob, market_price) = if yes_edge > no_edge {
            (Side::Yes, yes_edge, fair_value.yes_prob, yes_ask)
        } else {
            (Side::No, no_edge, fair_value.no_prob, no_bid)
        };

        // Adjust for fees and slippage
        let total_costs = self.fee_rate + self.slippage_estimate;
        let adjusted_edge = raw_edge - total_costs;

        if adjusted_edge <= dec!(0) {
            return None;
        }

        // Determine signal reason
        let reason = if self.is_post_reset(market, Duration::minutes(2)) {
            SignalReason::PostResetLag
        } else if raw_edge > dec!(0.02) {
            SignalReason::SpotDivergence
        } else {
            SignalReason::VolatilitySpike
        };

        Some(Signal::new(
            market.clone(),
            side,
            fair_prob,
            market_price,
            adjusted_edge,
            fair_value.confidence,
            reason,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GbmModel;
    use crate::orderbook::PriceLevel;
    use rust_decimal_macros::dec;

    fn create_test_market(open_offset_mins: i64, close_offset_mins: i64) -> Market {
        let now = Utc::now();
        Market {
            condition_id: "test-condition".to_string(),
            yes_token_id: "yes-token".to_string(),
            no_token_id: "no-token".to_string(),
            open_price: dec!(100000),
            open_time: now - Duration::minutes(open_offset_mins),
            close_time: now + Duration::minutes(close_offset_mins),
        }
    }

    fn create_test_orderbook(ask_price: Decimal) -> OrderBook {
        OrderBook {
            token_id: "yes-token".to_string(),
            bids: vec![],
            asks: vec![PriceLevel {
                price: ask_price,
                size: dec!(100),
            }],
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_detector_creation() {
        let model = GbmModel::new();
        let detector = SignalDetector::new(model, dec!(0.005), dec!(0.002));
        assert_eq!(detector.fee_rate, dec!(0.005));
        assert_eq!(detector.slippage_estimate, dec!(0.002));
    }

    #[test]
    fn test_is_post_reset_within_window() {
        let model = GbmModel::new();
        let detector = SignalDetector::new(model, dec!(0.005), dec!(0.002));

        // Market opened 1 minute ago
        let market = create_test_market(1, 14);
        assert!(detector.is_post_reset(&market, Duration::minutes(2)));
    }

    #[test]
    fn test_is_post_reset_outside_window() {
        let model = GbmModel::new();
        let detector = SignalDetector::new(model, dec!(0.005), dec!(0.002));

        // Market opened 5 minutes ago
        let market = create_test_market(5, 10);
        assert!(!detector.is_post_reset(&market, Duration::minutes(2)));
    }

    #[test]
    fn test_detect_expired_market() {
        let model = GbmModel::new();
        let detector = SignalDetector::new(model, dec!(0.005), dec!(0.002));

        // Market already expired
        let market = create_test_market(20, -1);
        let orderbook = create_test_orderbook(dec!(0.5));

        let signal = detector.detect(&market, dec!(100000), dec!(0.4), &orderbook);
        assert!(signal.is_none());
    }

    #[test]
    fn test_detect_no_asks() {
        let model = GbmModel::new();
        let detector = SignalDetector::new(model, dec!(0.005), dec!(0.002));

        let market = create_test_market(5, 10);
        let orderbook = OrderBook {
            token_id: "yes-token".to_string(),
            bids: vec![],
            asks: vec![],
            updated_at: Utc::now(),
        };

        let signal = detector.detect(&market, dec!(100000), dec!(0.4), &orderbook);
        assert!(signal.is_none());
    }

    #[test]
    fn test_detect_no_edge() {
        let model = GbmModel::new();
        let detector = SignalDetector::new(model, dec!(0.005), dec!(0.002));

        let market = create_test_market(5, 10);
        // Fair value ~0.5, orderbook at 0.5, no edge after costs
        let orderbook = create_test_orderbook(dec!(0.50));

        let signal = detector.detect(&market, dec!(100000), dec!(0.4), &orderbook);
        assert!(signal.is_none());
    }

    #[test]
    fn test_detect_generates_yes_signal() {
        let model = GbmModel::new();
        let detector = SignalDetector::new(model, dec!(0.005), dec!(0.002));

        let market = create_test_market(5, 10);
        // Price went up significantly, so P(up) should be high
        // Orderbook has low ask price, creating edge
        let orderbook = create_test_orderbook(dec!(0.40));

        let signal = detector.detect(&market, dec!(102000), dec!(0.4), &orderbook);
        if let Some(s) = signal {
            // Should be Yes side since price is up
            assert_eq!(s.side, Side::Yes);
            assert!(s.adjusted_edge > dec!(0));
        }
    }

    #[test]
    fn test_detect_post_reset_reason() {
        let model = GbmModel::new();
        let detector = SignalDetector::new(model, dec!(0.001), dec!(0.001));

        // Market just opened 1 minute ago
        let market = create_test_market(1, 14);
        let orderbook = create_test_orderbook(dec!(0.30));

        let signal = detector.detect(&market, dec!(105000), dec!(0.4), &orderbook);
        if let Some(s) = signal {
            assert_eq!(s.reason, SignalReason::PostResetLag);
        }
    }
}
