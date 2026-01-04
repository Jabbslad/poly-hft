//! Data recorder for tick capture

use crate::feed::PriceTick;
use crate::orderbook::OrderBook;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// Records market data to Parquet files
pub struct DataRecorder {
    output_dir: PathBuf,
    price_tx: mpsc::Sender<PriceTick>,
    orderbook_tx: mpsc::Sender<OrderBook>,
}

impl DataRecorder {
    /// Create a new data recorder
    pub fn new(output_dir: PathBuf) -> Self {
        let (price_tx, _price_rx) = mpsc::channel(10_000);
        let (orderbook_tx, _orderbook_rx) = mpsc::channel(10_000);

        // TODO: Spawn background writers

        Self {
            output_dir,
            price_tx,
            orderbook_tx,
        }
    }

    /// Record a price tick
    pub async fn record_price(&self, tick: PriceTick) -> anyhow::Result<()> {
        self.price_tx.send(tick).await?;
        Ok(())
    }

    /// Record an order book snapshot
    pub async fn record_orderbook(&self, book: OrderBook) -> anyhow::Result<()> {
        self.orderbook_tx.send(book).await?;
        Ok(())
    }

    /// Get output directory
    pub fn output_dir(&self) -> &PathBuf {
        &self.output_dir
    }
}
