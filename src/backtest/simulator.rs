//! Backtest simulator engine

use super::{BacktestConfig, BacktestResult, EventStream};

/// Runs backtest simulation
pub struct BacktestSimulator {
    config: BacktestConfig,
}

impl BacktestSimulator {
    /// Create a new simulator
    pub fn new(config: BacktestConfig) -> Self {
        Self { config }
    }

    /// Run the backtest
    pub async fn run(&self) -> anyhow::Result<BacktestResult> {
        let mut events = EventStream::new(
            self.config.data_dir.clone(),
            self.config.start_time,
            self.config.end_time,
        );

        // TODO: Process events through strategy
        for (_timestamp, _event) in &mut events {
            // Process event
        }

        // TODO: Return actual results
        Ok(BacktestResult::default())
    }
}
