//! Binance WebSocket price feed implementation

use super::{PriceFeed, PriceTick};
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Binance WebSocket feed for btcusdt@trade stream
pub struct BinanceFeed {
    symbol: String,
}

impl BinanceFeed {
    /// Create a new Binance feed for the given symbol
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
        }
    }
}

#[async_trait]
impl PriceFeed for BinanceFeed {
    async fn subscribe(&self) -> anyhow::Result<mpsc::Receiver<PriceTick>> {
        let (tx, rx) = mpsc::channel(1024);

        // TODO: Implement WebSocket connection to Binance
        // wss://stream.binance.com:9443/ws/{symbol}@trade
        tracing::info!("Subscribing to Binance {} feed", self.symbol);

        let _tx = tx; // Keep sender alive
        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binance_feed_creation() {
        let feed = BinanceFeed::new("btcusdt");
        assert_eq!(feed.symbol, "btcusdt");
    }
}
