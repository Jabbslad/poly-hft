//! Price feed module
//!
//! Provides real-time BTC price from Binance WebSocket

mod binance;
mod types;

pub use binance::BinanceFeed;
pub use types::PriceTick;

use async_trait::async_trait;
use tokio::sync::mpsc;

/// Trait for price feed implementations
#[async_trait]
pub trait PriceFeed: Send + Sync {
    /// Subscribe to price updates
    async fn subscribe(&self) -> anyhow::Result<mpsc::Receiver<PriceTick>>;
}
