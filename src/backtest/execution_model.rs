//! Queue position and fill simulation

use crate::execution::{Fill, OrderId};
use crate::orderbook::OrderBook;
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Queue state for a pending order
#[derive(Debug, Clone)]
pub struct QueueState {
    /// Price level
    pub price_level: Decimal,
    /// Size ahead in queue
    pub ahead_size: Decimal,
    /// Our order size
    pub our_size: Decimal,
    /// Amount filled so far
    pub filled: Decimal,
}

/// Simulates order queue position and fills
pub struct QueueSimulator {
    /// Simulated order latency in ms
    pub latency_ms: u64,
    /// Queue states by order ID
    pub queue_position: HashMap<OrderId, QueueState>,
}

impl QueueSimulator {
    /// Create a new queue simulator
    pub fn new(latency_ms: u64) -> Self {
        Self {
            latency_ms,
            queue_position: HashMap::new(),
        }
    }

    /// Process order book update and return any fills
    pub fn process_book_update(&mut self, _book: &OrderBook) -> Vec<Fill> {
        // TODO: Advance queue positions based on book changes
        vec![]
    }
}
