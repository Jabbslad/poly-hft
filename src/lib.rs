//! poly-hft: High-frequency trading bot for Polymarket BTC up/down markets
//!
//! This library provides the core components for:
//! - Real-time price feeds from Binance
//! - Market discovery via Gamma API
//! - Order book management from Polymarket WebSocket
//! - Fair value calculation using GBM model
//! - Signal generation and filtering
//! - Paper/live execution engine
//! - Risk management with Kelly criterion
//! - Data capture to Parquet
//! - Backtesting with queue simulation
//! - Full observability stack

pub mod backtest;
pub mod cli;
pub mod config;
pub mod data;
pub mod execution;
pub mod feed;
pub mod market;
pub mod model;
pub mod orderbook;
pub mod risk;
pub mod signal;
pub mod telemetry;
pub mod ws;
