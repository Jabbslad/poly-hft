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
