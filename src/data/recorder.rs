//! Data recorder for tick capture

use super::parquet::{OrderBookRecord, ParquetWriter, PriceTickRecord};
use crate::feed::PriceTick;
use crate::orderbook::OrderBook;
use chrono::{Duration, Utc};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Configuration for data recording
#[derive(Debug, Clone)]
pub struct RecorderConfig {
    /// Output directory for Parquet files
    pub output_dir: PathBuf,
    /// Rotation interval in seconds
    pub rotation_interval_secs: u64,
    /// Buffer size before flushing
    pub buffer_size: usize,
    /// Maximum time between flushes
    pub flush_interval_secs: u64,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("./data"),
            rotation_interval_secs: 3600, // 1 hour
            buffer_size: 1000,
            flush_interval_secs: 60,
        }
    }
}

/// Records market data to Parquet files
pub struct DataRecorder {
    config: RecorderConfig,
    price_tx: mpsc::Sender<PriceTick>,
    orderbook_tx: mpsc::Sender<OrderBook>,
    stats: Arc<RwLock<RecorderStats>>,
}

/// Recording statistics
#[derive(Debug, Default, Clone)]
pub struct RecorderStats {
    pub price_ticks_received: u64,
    pub price_ticks_written: u64,
    pub orderbook_updates_received: u64,
    pub orderbook_updates_written: u64,
    pub files_written: u64,
    pub last_flush: Option<chrono::DateTime<Utc>>,
}

impl DataRecorder {
    /// Create a new data recorder
    pub fn new(config: RecorderConfig) -> Self {
        let (price_tx, price_rx) = mpsc::channel(10_000);
        let (orderbook_tx, orderbook_rx) = mpsc::channel(10_000);
        let stats = Arc::new(RwLock::new(RecorderStats::default()));

        // Spawn price tick writer
        let price_writer =
            ParquetWriter::new(config.output_dir.clone(), config.rotation_interval_secs);
        let price_stats = stats.clone();
        let price_config = config.clone();
        tokio::spawn(async move {
            Self::run_price_writer(price_rx, price_writer, price_config, price_stats).await;
        });

        // Spawn orderbook writer
        let orderbook_writer =
            ParquetWriter::new(config.output_dir.clone(), config.rotation_interval_secs);
        let orderbook_stats = stats.clone();
        let orderbook_config = config.clone();
        tokio::spawn(async move {
            Self::run_orderbook_writer(
                orderbook_rx,
                orderbook_writer,
                orderbook_config,
                orderbook_stats,
            )
            .await;
        });

        Self {
            config,
            price_tx,
            orderbook_tx,
            stats,
        }
    }

    /// Create a new recorder with default config
    pub fn with_output_dir(output_dir: PathBuf) -> Self {
        let config = RecorderConfig {
            output_dir,
            ..Default::default()
        };
        Self::new(config)
    }

    /// Run the price tick writer task
    async fn run_price_writer(
        mut rx: mpsc::Receiver<PriceTick>,
        mut writer: ParquetWriter,
        config: RecorderConfig,
        stats: Arc<RwLock<RecorderStats>>,
    ) {
        let mut buffer: Vec<PriceTickRecord> = Vec::with_capacity(config.buffer_size);
        let mut last_flush = Utc::now();
        let flush_interval = Duration::seconds(config.flush_interval_secs as i64);

        loop {
            // Use timeout to ensure periodic flushing
            let timeout = tokio::time::Duration::from_secs(config.flush_interval_secs);

            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(tick) => {
                            {
                                let mut s = stats.write().await;
                                s.price_ticks_received += 1;
                            }

                            buffer.push(PriceTickRecord {
                                timestamp: tick.timestamp,
                                symbol: tick.symbol,
                                price: tick.price,
                                exchange_ts: tick.exchange_ts,
                            });

                            // Flush if buffer is full
                            if buffer.len() >= config.buffer_size {
                                Self::flush_price_buffer(&mut buffer, &mut writer, &stats).await;
                                last_flush = Utc::now();
                            }
                        }
                        None => {
                            // Channel closed, flush remaining and exit
                            if !buffer.is_empty() {
                                Self::flush_price_buffer(&mut buffer, &mut writer, &stats).await;
                            }
                            tracing::info!("Price writer shutting down");
                            break;
                        }
                    }
                }

                _ = tokio::time::sleep(timeout) => {
                    // Periodic flush
                    let now = Utc::now();
                    if now - last_flush >= flush_interval && !buffer.is_empty() {
                        Self::flush_price_buffer(&mut buffer, &mut writer, &stats).await;
                        last_flush = now;
                    }
                }
            }
        }
    }

    /// Flush price tick buffer to disk
    async fn flush_price_buffer(
        buffer: &mut Vec<PriceTickRecord>,
        writer: &mut ParquetWriter,
        stats: &Arc<RwLock<RecorderStats>>,
    ) {
        if buffer.is_empty() {
            return;
        }

        let now = Utc::now();

        // Check for rotation
        if writer.needs_rotation(now) {
            writer.mark_rotation(now);
        }

        let path = writer.file_path("price_ticks", now);
        let count = buffer.len();

        match writer.write_price_ticks(&path, buffer) {
            Ok(()) => {
                let mut s = stats.write().await;
                s.price_ticks_written += count as u64;
                s.files_written += 1;
                s.last_flush = Some(now);
                tracing::debug!(count, path = ?path, "Flushed price ticks");
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to write price ticks");
            }
        }

        buffer.clear();
    }

    /// Run the orderbook writer task
    async fn run_orderbook_writer(
        mut rx: mpsc::Receiver<OrderBook>,
        mut writer: ParquetWriter,
        config: RecorderConfig,
        stats: Arc<RwLock<RecorderStats>>,
    ) {
        let mut buffer: Vec<OrderBookRecord> = Vec::with_capacity(config.buffer_size);
        let mut last_flush = Utc::now();
        let flush_interval = Duration::seconds(config.flush_interval_secs as i64);

        loop {
            let timeout = tokio::time::Duration::from_secs(config.flush_interval_secs);

            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(book) => {
                            {
                                let mut s = stats.write().await;
                                s.orderbook_updates_received += 1;
                            }

                            buffer.push(OrderBookRecord {
                                timestamp: book.updated_at,
                                token_id: book.token_id.clone(),
                                bids: book.bids.iter().map(|l| (l.price, l.size)).collect(),
                                asks: book.asks.iter().map(|l| (l.price, l.size)).collect(),
                            });

                            if buffer.len() >= config.buffer_size {
                                Self::flush_orderbook_buffer(&mut buffer, &mut writer, &stats).await;
                                last_flush = Utc::now();
                            }
                        }
                        None => {
                            if !buffer.is_empty() {
                                Self::flush_orderbook_buffer(&mut buffer, &mut writer, &stats).await;
                            }
                            tracing::info!("Orderbook writer shutting down");
                            break;
                        }
                    }
                }

                _ = tokio::time::sleep(timeout) => {
                    let now = Utc::now();
                    if now - last_flush >= flush_interval && !buffer.is_empty() {
                        Self::flush_orderbook_buffer(&mut buffer, &mut writer, &stats).await;
                        last_flush = now;
                    }
                }
            }
        }
    }

    /// Flush orderbook buffer to disk
    async fn flush_orderbook_buffer(
        buffer: &mut Vec<OrderBookRecord>,
        writer: &mut ParquetWriter,
        stats: &Arc<RwLock<RecorderStats>>,
    ) {
        if buffer.is_empty() {
            return;
        }

        let now = Utc::now();

        if writer.needs_rotation(now) {
            writer.mark_rotation(now);
        }

        let path = writer.file_path("orderbook", now);
        let count = buffer.len();

        match writer.write_orderbook_snapshots(&path, buffer) {
            Ok(()) => {
                let mut s = stats.write().await;
                s.orderbook_updates_written += count as u64;
                s.files_written += 1;
                s.last_flush = Some(now);
                tracing::debug!(count, path = ?path, "Flushed orderbook snapshots");
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to write orderbook snapshots");
            }
        }

        buffer.clear();
    }

    /// Record a price tick
    pub async fn record_price(&self, tick: PriceTick) -> anyhow::Result<()> {
        self.price_tx
            .send(tick)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send price tick: {}", e))?;
        Ok(())
    }

    /// Record an order book snapshot
    pub async fn record_orderbook(&self, book: OrderBook) -> anyhow::Result<()> {
        self.orderbook_tx
            .send(book)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send orderbook: {}", e))?;
        Ok(())
    }

    /// Get output directory
    pub fn output_dir(&self) -> &PathBuf {
        &self.config.output_dir
    }

    /// Get current statistics
    pub async fn stats(&self) -> RecorderStats {
        self.stats.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orderbook::PriceLevel;
    use rust_decimal_macros::dec;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_recorder_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = RecorderConfig {
            output_dir: temp_dir.path().to_path_buf(),
            rotation_interval_secs: 3600,
            buffer_size: 10,
            flush_interval_secs: 1,
        };

        let recorder = DataRecorder::new(config);
        assert_eq!(recorder.output_dir(), temp_dir.path());
    }

    #[tokio::test]
    async fn test_record_price_tick() {
        let temp_dir = TempDir::new().unwrap();
        let config = RecorderConfig {
            output_dir: temp_dir.path().to_path_buf(),
            rotation_interval_secs: 3600,
            buffer_size: 1, // Flush immediately
            flush_interval_secs: 1,
        };

        let recorder = DataRecorder::new(config);

        let tick = PriceTick {
            symbol: "BTCUSDT".to_string(),
            price: dec!(42500.00),
            timestamp: Utc::now(),
            exchange_ts: Utc::now(),
        };

        recorder.record_price(tick).await.unwrap();

        // Give time for async flush
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let stats = recorder.stats().await;
        assert_eq!(stats.price_ticks_received, 1);
    }

    #[tokio::test]
    async fn test_record_orderbook() {
        let temp_dir = TempDir::new().unwrap();
        let config = RecorderConfig {
            output_dir: temp_dir.path().to_path_buf(),
            rotation_interval_secs: 3600,
            buffer_size: 1,
            flush_interval_secs: 1,
        };

        let recorder = DataRecorder::new(config);

        let book = OrderBook {
            token_id: "token123".to_string(),
            bids: vec![PriceLevel {
                price: dec!(0.55),
                size: dec!(100),
            }],
            asks: vec![PriceLevel {
                price: dec!(0.56),
                size: dec!(100),
            }],
            updated_at: Utc::now(),
        };

        recorder.record_orderbook(book).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let stats = recorder.stats().await;
        assert_eq!(stats.orderbook_updates_received, 1);
    }

    #[test]
    fn test_default_config() {
        let config = RecorderConfig::default();
        assert_eq!(config.rotation_interval_secs, 3600);
        assert_eq!(config.buffer_size, 1000);
        assert_eq!(config.flush_interval_secs, 60);
    }
}
