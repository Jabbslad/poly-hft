//! CLI interface for poly-hft
//!
//! Provides subcommands for:
//! - `run`: Start paper trading
//! - `capture`: Data capture only (no trading)
//! - `backtest`: Run backtest on captured data
//! - `status`: Show current state
//! - `config`: Show/edit configuration

mod backtest;
mod capture;
mod run;

pub use backtest::BacktestArgs;
pub use capture::CaptureArgs;
pub use run::RunArgs;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "poly-hft")]
#[command(about = "High-frequency trading bot for Polymarket BTC up/down markets")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Path to configuration file
    #[arg(short, long, default_value = "config.toml")]
    pub config: String,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start paper trading
    Run(RunArgs),
    /// Data capture only (no trading)
    Capture(CaptureArgs),
    /// Run backtest on captured data
    Backtest(BacktestArgs),
    /// Show current state
    Status,
    /// Show/edit configuration
    Config,
}
