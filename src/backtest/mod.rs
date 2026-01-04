//! Backtesting module
//!
//! Replays historical data with realistic execution simulation

mod analytics;
mod execution_model;
mod replay;
mod simulator;

pub use analytics::{BacktestResult, BacktestSummary};
pub use execution_model::QueueSimulator;
pub use replay::{BacktestEvent, EventStream};
pub use simulator::BacktestSimulator;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::path::PathBuf;

/// Backtest configuration
#[derive(Debug, Clone)]
pub struct BacktestConfig {
    /// Directory containing Parquet data files
    pub data_dir: PathBuf,
    /// Start time filter
    pub start_time: Option<DateTime<Utc>>,
    /// End time filter
    pub end_time: Option<DateTime<Utc>>,
    /// Initial capital
    pub initial_capital: Decimal,
    /// Simulated order latency in ms
    pub latency_ms: u64,
    /// Fee rate
    pub fee_rate: Decimal,
}
