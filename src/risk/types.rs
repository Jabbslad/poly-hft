//! Risk management types

use super::HaltReason;
use rust_decimal::Decimal;
use thiserror::Error;

/// Risk management errors
#[derive(Debug, Error)]
pub enum RiskError {
    /// Position size exceeds limit
    #[error("Position too large: {0}")]
    PositionTooLarge(Decimal),
    /// Maximum concurrent positions reached
    #[error("Maximum positions reached")]
    MaxPositionsReached,
    /// Maximum exposure reached
    #[error("Maximum exposure reached")]
    MaxExposureReached,
    /// Trading has been halted
    #[error("Trading halted: {0:?}")]
    TradingHalted(HaltReason),
}
