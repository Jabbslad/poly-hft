//! Market tracker implementation

use super::{GammaClient, Market, MarketTracker};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Tracks active markets with periodic refresh
pub struct MarketTrackerImpl {
    client: GammaClient,
    markets: Arc<RwLock<Vec<Market>>>,
}

impl MarketTrackerImpl {
    /// Create a new market tracker
    pub fn new(client: GammaClient) -> Self {
        Self {
            client,
            markets: Arc::new(RwLock::new(vec![])),
        }
    }
}

#[async_trait]
impl MarketTracker for MarketTrackerImpl {
    async fn get_active_markets(&self) -> anyhow::Result<Vec<Market>> {
        let markets = self.markets.read().await;
        Ok(markets.clone())
    }

    async fn refresh(&self) -> anyhow::Result<()> {
        let new_markets = self.client.fetch_btc_markets().await?;
        let mut markets = self.markets.write().await;
        *markets = new_markets;
        Ok(())
    }
}
