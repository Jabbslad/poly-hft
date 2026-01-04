//! Risk management module
//!
//! Position sizing, limits, and risk controls

mod kelly;
mod limits;
mod position;
mod types;

pub use kelly::KellyCalculator;
pub use limits::{DrawdownMonitor, HaltReason, PositionLimits};
pub use position::{ClosedPosition, Position, PositionTracker};
pub use types::RiskError;

use crate::execution::Order;
use crate::signal::Signal;
use rust_decimal::Decimal;

/// Trait for risk management implementations
pub trait RiskManager: Send + Sync {
    /// Calculate position size for a signal
    fn calculate_size(&self, signal: &Signal, bankroll: Decimal) -> Decimal;
    /// Check if order passes risk limits
    fn check_limits(&self, order: &Order, tracker: &PositionTracker) -> Result<(), RiskError>;
    /// Check if trading should be halted
    fn should_halt(&self) -> Option<HaltReason>;
}
