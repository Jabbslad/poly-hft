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
