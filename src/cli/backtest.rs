//! Backtest command implementation

use clap::Args;
use rust_decimal::Decimal;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct BacktestArgs {
    /// Directory containing Parquet files
    #[arg(long, default_value = "./data")]
    pub data_dir: PathBuf,

    /// Start time filter (ISO 8601)
    #[arg(long)]
    pub start: Option<String>,

    /// End time filter (ISO 8601)
    #[arg(long)]
    pub end: Option<String>,

    /// Initial capital
    #[arg(long)]
    pub capital: Option<Decimal>,

    /// Simulated latency in ms
    #[arg(long, default_value = "50")]
    pub latency: u64,

    /// Output directory for results
    #[arg(long, default_value = "./output")]
    pub output: PathBuf,

    /// Output format: json or table
    #[arg(long, default_value = "table")]
    pub format: String,
}

impl BacktestArgs {
    pub async fn execute(&self) -> anyhow::Result<()> {
        // TODO: Implement backtesting
        tracing::info!("Running backtest on {:?}...", self.data_dir);
        Ok(())
    }
}
