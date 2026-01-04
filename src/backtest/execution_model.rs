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

    /// Add an order to the queue
    pub fn add_order(
        &mut self,
        order_id: OrderId,
        price_level: Decimal,
        size: Decimal,
        ahead_size: Decimal,
    ) {
        self.queue_position.insert(
            order_id,
            QueueState {
                price_level,
                ahead_size,
                our_size: size,
                filled: Decimal::ZERO,
            },
        );
    }

    /// Get queue state for an order
    pub fn get_queue_state(&self, order_id: &OrderId) -> Option<&QueueState> {
        self.queue_position.get(order_id)
    }

    /// Remove an order from tracking
    pub fn remove_order(&mut self, order_id: &OrderId) {
        self.queue_position.remove(order_id);
    }

    /// Process order book update and return any fills
    pub fn process_book_update(&mut self, _book: &OrderBook) -> Vec<Fill> {
        // TODO: Advance queue positions based on book changes
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    #[test]
    fn test_queue_simulator_creation() {
        let sim = QueueSimulator::new(50);
        assert_eq!(sim.latency_ms, 50);
        assert!(sim.queue_position.is_empty());
    }

    #[test]
    fn test_queue_state_creation() {
        let state = QueueState {
            price_level: dec!(0.55),
            ahead_size: dec!(1000),
            our_size: dec!(100),
            filled: dec!(0),
        };
        assert_eq!(state.price_level, dec!(0.55));
        assert_eq!(state.ahead_size, dec!(1000));
        assert_eq!(state.our_size, dec!(100));
        assert_eq!(state.filled, dec!(0));
    }

    #[test]
    fn test_add_order_to_queue() {
        let mut sim = QueueSimulator::new(50);
        let order_id = Uuid::new_v4();

        sim.add_order(order_id, dec!(0.55), dec!(100), dec!(500));

        let state = sim.get_queue_state(&order_id).unwrap();
        assert_eq!(state.price_level, dec!(0.55));
        assert_eq!(state.our_size, dec!(100));
        assert_eq!(state.ahead_size, dec!(500));
    }

    #[test]
    fn test_remove_order_from_queue() {
        let mut sim = QueueSimulator::new(50);
        let order_id = Uuid::new_v4();

        sim.add_order(order_id, dec!(0.55), dec!(100), dec!(500));
        assert!(sim.get_queue_state(&order_id).is_some());

        sim.remove_order(&order_id);
        assert!(sim.get_queue_state(&order_id).is_none());
    }

    #[test]
    fn test_process_book_update_returns_empty() {
        use crate::orderbook::OrderBook;
        use chrono::Utc;

        let mut sim = QueueSimulator::new(50);
        let book = OrderBook {
            token_id: "token".to_string(),
            bids: vec![],
            asks: vec![],
            updated_at: Utc::now(),
        };

        let fills = sim.process_book_update(&book);
        assert!(fills.is_empty());
    }

    #[test]
    fn test_queue_state_clone() {
        let state = QueueState {
            price_level: dec!(0.55),
            ahead_size: dec!(1000),
            our_size: dec!(100),
            filled: dec!(25),
        };

        let cloned = state.clone();
        assert_eq!(state.price_level, cloned.price_level);
        assert_eq!(state.filled, cloned.filled);
    }
}
