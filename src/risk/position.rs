//! Position tracking

use crate::execution::Fill;
use crate::market::Market;
use crate::signal::{Side, Signal};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// An open position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// Position identifier
    pub id: Uuid,
    /// Associated market
    pub market: Market,
    /// Trade side
    pub side: Side,
    /// Entry price
    pub entry_price: Decimal,
    /// Position size
    pub size: Decimal,
    /// Entry timestamp
    pub entry_time: DateTime<Utc>,
    /// Current unrealized P&L
    pub unrealized_pnl: Decimal,
}

/// A closed position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosedPosition {
    /// Original position
    pub position: Position,
    /// Exit price
    pub exit_price: Decimal,
    /// Exit timestamp
    pub exit_time: DateTime<Utc>,
    /// Realized P&L
    pub realized_pnl: Decimal,
    /// Total fees paid
    pub fees: Decimal,
}

/// Tracks all positions
pub struct PositionTracker {
    /// Open positions by ID
    pub open_positions: HashMap<Uuid, Position>,
    /// Closed position history
    pub closed_positions: Vec<ClosedPosition>,
    /// Total capital at risk
    pub total_exposure: Decimal,
}

impl PositionTracker {
    /// Create a new position tracker
    pub fn new() -> Self {
        Self {
            open_positions: HashMap::new(),
            closed_positions: vec![],
            total_exposure: dec!(0),
        }
    }

    /// Open a new position from a signal and fill
    pub fn open(&mut self, signal: &Signal, fill: &Fill) -> Position {
        let position = Position {
            id: Uuid::new_v4(),
            market: signal.market.clone(),
            side: signal.side,
            entry_price: fill.price,
            size: fill.size,
            entry_time: fill.timestamp,
            unrealized_pnl: dec!(0),
        };

        self.total_exposure += fill.size * fill.price;
        self.open_positions.insert(position.id, position.clone());
        position
    }

    /// Close a position
    pub fn close(&mut self, position_id: Uuid, fill: &Fill) -> Option<ClosedPosition> {
        let position = self.open_positions.remove(&position_id)?;

        // Calculate P&L
        let pnl = match position.side {
            Side::Yes => (fill.price - position.entry_price) * position.size,
            Side::No => (position.entry_price - fill.price) * position.size,
        };

        let closed = ClosedPosition {
            exit_price: fill.price,
            exit_time: fill.timestamp,
            realized_pnl: pnl - fill.fees,
            fees: fill.fees,
            position,
        };

        self.total_exposure -= fill.size * fill.price;
        self.closed_positions.push(closed.clone());
        Some(closed)
    }

    /// Update mark-to-market for open positions
    pub fn update_mark(&mut self, market_id: &str, current_price: Decimal) {
        for position in self.open_positions.values_mut() {
            if position.market.condition_id == market_id {
                position.unrealized_pnl = match position.side {
                    Side::Yes => (current_price - position.entry_price) * position.size,
                    Side::No => (position.entry_price - current_price) * position.size,
                };
            }
        }
    }

    /// Get total P&L (realized + unrealized)
    pub fn total_pnl(&self) -> Decimal {
        let realized: Decimal = self.closed_positions.iter().map(|p| p.realized_pnl).sum();
        let unrealized: Decimal = self.open_positions.values().map(|p| p.unrealized_pnl).sum();
        realized + unrealized
    }

    /// Get number of open positions
    pub fn open_count(&self) -> usize {
        self.open_positions.len()
    }
}

impl Default for PositionTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::Fill;
    use crate::signal::SignalReason;
    use chrono::Duration;

    fn create_test_market() -> Market {
        Market {
            condition_id: "test-cond-123".to_string(),
            yes_token_id: "yes-token".to_string(),
            no_token_id: "no-token".to_string(),
            open_price: dec!(100000),
            open_time: Utc::now() - Duration::minutes(5),
            close_time: Utc::now() + Duration::minutes(10),
        }
    }

    fn create_test_signal(side: Side) -> Signal {
        Signal::new(
            create_test_market(),
            side,
            dec!(0.55),
            dec!(0.50),
            dec!(0.02),
            dec!(0.8),
            SignalReason::SpotDivergence,
        )
    }

    fn create_test_fill(price: Decimal, size: Decimal, fees: Decimal) -> Fill {
        Fill {
            order_id: Uuid::new_v4(),
            token_id: "yes-token".to_string(),
            side: Side::Yes,
            price,
            size,
            timestamp: Utc::now(),
            fees,
        }
    }

    #[test]
    fn test_position_tracker_creation() {
        let tracker = PositionTracker::new();
        assert_eq!(tracker.open_count(), 0);
        assert_eq!(tracker.total_exposure, dec!(0));
        assert!(tracker.closed_positions.is_empty());
    }

    #[test]
    fn test_position_tracker_default() {
        let tracker = PositionTracker::default();
        assert_eq!(tracker.open_count(), 0);
    }

    #[test]
    fn test_open_position() {
        let mut tracker = PositionTracker::new();
        let signal = create_test_signal(Side::Yes);
        let fill = create_test_fill(dec!(0.50), dec!(100), dec!(0.5));

        let position = tracker.open(&signal, &fill);

        assert_eq!(position.side, Side::Yes);
        assert_eq!(position.entry_price, dec!(0.50));
        assert_eq!(position.size, dec!(100));
        assert_eq!(position.unrealized_pnl, dec!(0));
        assert_eq!(tracker.open_count(), 1);
        assert_eq!(tracker.total_exposure, dec!(50)); // 100 * 0.50
    }

    #[test]
    fn test_close_position_yes_profit() {
        let mut tracker = PositionTracker::new();
        let signal = create_test_signal(Side::Yes);
        let entry_fill = create_test_fill(dec!(0.50), dec!(100), dec!(0.5));

        let position = tracker.open(&signal, &entry_fill);
        let position_id = position.id;

        // Exit at higher price (profit for Yes side)
        let exit_fill = create_test_fill(dec!(0.60), dec!(100), dec!(0.5));
        let closed = tracker.close(position_id, &exit_fill).unwrap();

        // P&L = (0.60 - 0.50) * 100 - fees = 10 - 0.5 = 9.5
        assert_eq!(closed.exit_price, dec!(0.60));
        assert_eq!(closed.realized_pnl, dec!(9.5));
        assert_eq!(tracker.open_count(), 0);
    }

    #[test]
    fn test_close_position_yes_loss() {
        let mut tracker = PositionTracker::new();
        let signal = create_test_signal(Side::Yes);
        let entry_fill = create_test_fill(dec!(0.50), dec!(100), dec!(0.5));

        let position = tracker.open(&signal, &entry_fill);
        let position_id = position.id;

        // Exit at lower price (loss for Yes side)
        let exit_fill = create_test_fill(dec!(0.40), dec!(100), dec!(0.5));
        let closed = tracker.close(position_id, &exit_fill).unwrap();

        // P&L = (0.40 - 0.50) * 100 - fees = -10 - 0.5 = -10.5
        assert_eq!(closed.realized_pnl, dec!(-10.5));
    }

    #[test]
    fn test_close_position_no_side() {
        let mut tracker = PositionTracker::new();
        let signal = create_test_signal(Side::No);
        let entry_fill = Fill {
            order_id: Uuid::new_v4(),
            token_id: "no-token".to_string(),
            side: Side::No,
            price: dec!(0.50),
            size: dec!(100),
            timestamp: Utc::now(),
            fees: dec!(0.5),
        };

        let position = tracker.open(&signal, &entry_fill);
        let position_id = position.id;

        // Exit at lower price (profit for No side - price went down)
        let exit_fill = Fill {
            order_id: Uuid::new_v4(),
            token_id: "no-token".to_string(),
            side: Side::No,
            price: dec!(0.40),
            size: dec!(100),
            timestamp: Utc::now(),
            fees: dec!(0.5),
        };
        let closed = tracker.close(position_id, &exit_fill).unwrap();

        // P&L for No = (entry - exit) * size - fees = (0.50 - 0.40) * 100 - 0.5 = 9.5
        assert_eq!(closed.realized_pnl, dec!(9.5));
    }

    #[test]
    fn test_close_nonexistent_position() {
        let mut tracker = PositionTracker::new();
        let fill = create_test_fill(dec!(0.50), dec!(100), dec!(0.5));
        let result = tracker.close(Uuid::new_v4(), &fill);
        assert!(result.is_none());
    }

    #[test]
    fn test_update_mark() {
        let mut tracker = PositionTracker::new();
        let signal = create_test_signal(Side::Yes);
        let fill = create_test_fill(dec!(0.50), dec!(100), dec!(0.5));

        let position = tracker.open(&signal, &fill);
        let position_id = position.id;

        // Update mark to higher price
        tracker.update_mark("test-cond-123", dec!(0.60));

        let updated_position = tracker.open_positions.get(&position_id).unwrap();
        // Unrealized P&L = (0.60 - 0.50) * 100 = 10
        assert_eq!(updated_position.unrealized_pnl, dec!(10));
    }

    #[test]
    fn test_update_mark_no_side() {
        let mut tracker = PositionTracker::new();
        let signal = create_test_signal(Side::No);
        let fill = Fill {
            order_id: Uuid::new_v4(),
            token_id: "no-token".to_string(),
            side: Side::No,
            price: dec!(0.50),
            size: dec!(100),
            timestamp: Utc::now(),
            fees: dec!(0.5),
        };

        let position = tracker.open(&signal, &fill);
        let position_id = position.id;

        // Update mark to lower price (profit for No side)
        tracker.update_mark("test-cond-123", dec!(0.40));

        let updated_position = tracker.open_positions.get(&position_id).unwrap();
        // Unrealized P&L for No = (entry - current) * size = (0.50 - 0.40) * 100 = 10
        assert_eq!(updated_position.unrealized_pnl, dec!(10));
    }

    #[test]
    fn test_update_mark_different_market() {
        let mut tracker = PositionTracker::new();
        let signal = create_test_signal(Side::Yes);
        let fill = create_test_fill(dec!(0.50), dec!(100), dec!(0.5));

        let position = tracker.open(&signal, &fill);
        let position_id = position.id;

        // Update mark for a different market
        tracker.update_mark("other-market", dec!(0.60));

        let updated_position = tracker.open_positions.get(&position_id).unwrap();
        // Should remain unchanged
        assert_eq!(updated_position.unrealized_pnl, dec!(0));
    }

    #[test]
    fn test_total_pnl() {
        let mut tracker = PositionTracker::new();
        let signal = create_test_signal(Side::Yes);

        // Open and close first position with profit
        let fill1 = create_test_fill(dec!(0.50), dec!(100), dec!(0.5));
        let pos1 = tracker.open(&signal, &fill1);
        let exit1 = create_test_fill(dec!(0.60), dec!(100), dec!(0.5));
        tracker.close(pos1.id, &exit1);

        // Open second position and update mark
        let fill2 = create_test_fill(dec!(0.50), dec!(100), dec!(0.5));
        tracker.open(&signal, &fill2);
        tracker.update_mark("test-cond-123", dec!(0.55));

        // Total P&L = 9.5 (realized) + 5 (unrealized) = 14.5
        assert_eq!(tracker.total_pnl(), dec!(14.5));
    }

    #[test]
    fn test_position_clone() {
        let position = Position {
            id: Uuid::new_v4(),
            market: create_test_market(),
            side: Side::Yes,
            entry_price: dec!(0.50),
            size: dec!(100),
            entry_time: Utc::now(),
            unrealized_pnl: dec!(5),
        };

        let cloned = position.clone();
        assert_eq!(position.id, cloned.id);
        assert_eq!(position.entry_price, cloned.entry_price);
        assert_eq!(position.unrealized_pnl, cloned.unrealized_pnl);
    }

    #[test]
    fn test_closed_position_clone() {
        let position = Position {
            id: Uuid::new_v4(),
            market: create_test_market(),
            side: Side::Yes,
            entry_price: dec!(0.50),
            size: dec!(100),
            entry_time: Utc::now(),
            unrealized_pnl: dec!(0),
        };

        let closed = ClosedPosition {
            position,
            exit_price: dec!(0.60),
            exit_time: Utc::now(),
            realized_pnl: dec!(10),
            fees: dec!(1),
        };

        let cloned = closed.clone();
        assert_eq!(closed.exit_price, cloned.exit_price);
        assert_eq!(closed.realized_pnl, cloned.realized_pnl);
    }
}
