//! Data recorder for tick capture

use super::parquet::{OrderBookRecord, ParquetWriter, PriceTickRecord};
use crate::feed::PriceTick;
use crate::orderbook::OrderBook;
use chrono::{Duration, Utc};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

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
            buffer_size: 100,        // Flush every 100 records
            flush_interval_secs: 10,  // Or every 10 seconds
        }
    }
}

/// Atomic recording statistics - lock-free for high performance
#[derive(Debug, Default)]
pub struct AtomicRecorderStats {
    pub price_ticks_received: AtomicU64,
    pub price_ticks_written: AtomicU64,
    pub orderbook_updates_received: AtomicU64,
    pub orderbook_updates_written: AtomicU64,
    pub files_written: AtomicU64,
    pub channel_drops: AtomicU64,
}

impl AtomicRecorderStats {
    /// Get a snapshot of current stats
    pub fn snapshot(&self) -> RecorderStats {
        RecorderStats {
            price_ticks_received: self.price_ticks_received.load(Ordering::Relaxed),
            price_ticks_written: self.price_ticks_written.load(Ordering::Relaxed),
            orderbook_updates_received: self.orderbook_updates_received.load(Ordering::Relaxed),
            orderbook_updates_written: self.orderbook_updates_written.load(Ordering::Relaxed),
            files_written: self.files_written.load(Ordering::Relaxed),
            channel_drops: self.channel_drops.load(Ordering::Relaxed),
        }
    }
}

/// Recording statistics snapshot
#[derive(Debug, Default, Clone)]
pub struct RecorderStats {
    pub price_ticks_received: u64,
    pub price_ticks_written: u64,
    pub orderbook_updates_received: u64,
    pub orderbook_updates_written: u64,
    pub files_written: u64,
    pub channel_drops: u64,
}

/// Records market data to Parquet files
pub struct DataRecorder {
    config: RecorderConfig,
    price_tx: mpsc::Sender<PriceTickRecord>,
    orderbook_tx: mpsc::Sender<OrderBookRecord>,
    stats: Arc<AtomicRecorderStats>,
}

impl DataRecorder {
    /// Create a new data recorder
    pub fn new(config: RecorderConfig) -> Self {
        let (price_tx, price_rx) = mpsc::channel(10_000);
        let (orderbook_tx, orderbook_rx) = mpsc::channel(10_000);
        let stats = Arc::new(AtomicRecorderStats::default());

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
        mut rx: mpsc::Receiver<PriceTickRecord>,
        mut writer: ParquetWriter,
        config: RecorderConfig,
        stats: Arc<AtomicRecorderStats>,
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
                            // Atomic increment - no lock needed
                            stats.price_ticks_received.fetch_add(1, Ordering::Relaxed);

                            buffer.push(tick);

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

    /// Flush price tick buffer to disk using async spawn_blocking
    async fn flush_price_buffer(
        buffer: &mut Vec<PriceTickRecord>,
        writer: &mut ParquetWriter,
        stats: &Arc<AtomicRecorderStats>,
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

        // Take ownership of buffer data for async write
        let ticks = std::mem::take(buffer);

        // Use async write with spawn_blocking
        match writer.write_price_ticks_async(path.clone(), ticks).await {
            Ok(()) => {
                stats
                    .price_ticks_written
                    .fetch_add(count as u64, Ordering::Relaxed);
                stats.files_written.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(count, path = ?path, "Flushed price ticks");
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to write price ticks");
            }
        }
    }

    /// Run the orderbook writer task
    async fn run_orderbook_writer(
        mut rx: mpsc::Receiver<OrderBookRecord>,
        mut writer: ParquetWriter,
        config: RecorderConfig,
        stats: Arc<AtomicRecorderStats>,
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
                            stats.orderbook_updates_received.fetch_add(1, Ordering::Relaxed);
                            buffer.push(book);

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

    /// Flush orderbook buffer to disk using async spawn_blocking
    async fn flush_orderbook_buffer(
        buffer: &mut Vec<OrderBookRecord>,
        writer: &mut ParquetWriter,
        stats: &Arc<AtomicRecorderStats>,
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

        // Take ownership for async write
        let snapshots = std::mem::take(buffer);

        match writer
            .write_orderbook_snapshots_async(path.clone(), snapshots)
            .await
        {
            Ok(()) => {
                stats
                    .orderbook_updates_written
                    .fetch_add(count as u64, Ordering::Relaxed);
                stats.files_written.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(count, path = ?path, "Flushed orderbook snapshots");
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to write orderbook snapshots");
            }
        }
    }

    /// Record a price tick - non-blocking using try_send
    pub fn record_price(&self, tick: PriceTick) -> Result<(), RecordError> {
        let record = PriceTickRecord {
            timestamp: tick.timestamp,
            symbol: Arc::from(tick.symbol.as_str()),
            price: tick.price,
            exchange_ts: tick.exchange_ts,
        };

        match self.price_tx.try_send(record) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.stats.channel_drops.fetch_add(1, Ordering::Relaxed);
                Err(RecordError::ChannelFull)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(RecordError::ChannelClosed),
        }
    }

    /// Record a price tick - async version that waits if channel is full
    pub async fn record_price_async(&self, tick: PriceTick) -> anyhow::Result<()> {
        let record = PriceTickRecord {
            timestamp: tick.timestamp,
            symbol: Arc::from(tick.symbol.as_str()),
            price: tick.price,
            exchange_ts: tick.exchange_ts,
        };

        self.price_tx
            .send(record)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send price tick: {}", e))
    }

    /// Record an order book snapshot - non-blocking using try_send
    pub fn record_orderbook(&self, book: OrderBook) -> Result<(), RecordError> {
        let record = OrderBookRecord {
            timestamp: book.updated_at,
            token_id: Arc::from(book.token_id.as_str()),
            bids: book.bids.iter().map(|l| (l.price, l.size)).collect(),
            asks: book.asks.iter().map(|l| (l.price, l.size)).collect(),
        };

        match self.orderbook_tx.try_send(record) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.stats.channel_drops.fetch_add(1, Ordering::Relaxed);
                Err(RecordError::ChannelFull)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(RecordError::ChannelClosed),
        }
    }

    /// Record an order book snapshot - async version
    pub async fn record_orderbook_async(&self, book: OrderBook) -> anyhow::Result<()> {
        let record = OrderBookRecord {
            timestamp: book.updated_at,
            token_id: Arc::from(book.token_id.as_str()),
            bids: book.bids.iter().map(|l| (l.price, l.size)).collect(),
            asks: book.asks.iter().map(|l| (l.price, l.size)).collect(),
        };

        self.orderbook_tx
            .send(record)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send orderbook: {}", e))
    }

    /// Get output directory
    pub fn output_dir(&self) -> &PathBuf {
        &self.config.output_dir
    }

    /// Get current statistics (lock-free snapshot)
    pub fn stats(&self) -> RecorderStats {
        self.stats.snapshot()
    }
}

/// Error type for recording operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordError {
    /// Channel is full - data was dropped
    ChannelFull,
    /// Channel is closed - recorder is shutting down
    ChannelClosed,
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordError::ChannelFull => write!(f, "Channel full, data dropped"),
            RecordError::ChannelClosed => write!(f, "Channel closed"),
        }
    }
}

impl std::error::Error for RecordError {}

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

        // Use non-blocking record
        recorder.record_price(tick).unwrap();

        // Give time for async flush
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let stats = recorder.stats();
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

        recorder.record_orderbook(book).unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let stats = recorder.stats();
        assert_eq!(stats.orderbook_updates_received, 1);
    }

    #[test]
    fn test_default_config() {
        let config = RecorderConfig::default();
        assert_eq!(config.rotation_interval_secs, 3600);
        assert_eq!(config.buffer_size, 100);
        assert_eq!(config.flush_interval_secs, 10);
    }

    #[tokio::test]
    async fn test_atomic_stats() {
        let stats = Arc::new(AtomicRecorderStats::default());

        // Simulate concurrent updates
        let stats_clone = stats.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..1000 {
                stats_clone
                    .price_ticks_received
                    .fetch_add(1, Ordering::Relaxed);
            }
        });

        for _ in 0..1000 {
            stats.price_ticks_received.fetch_add(1, Ordering::Relaxed);
        }

        handle.await.unwrap();

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.price_ticks_received, 2000);
    }

    #[tokio::test]
    async fn test_with_output_dir() {
        let temp_dir = TempDir::new().unwrap();
        let recorder = DataRecorder::with_output_dir(temp_dir.path().to_path_buf());
        assert_eq!(recorder.output_dir(), temp_dir.path());
    }

    #[test]
    fn test_record_error_display() {
        let full_error = RecordError::ChannelFull;
        assert_eq!(format!("{}", full_error), "Channel full, data dropped");

        let closed_error = RecordError::ChannelClosed;
        assert_eq!(format!("{}", closed_error), "Channel closed");
    }

    #[test]
    fn test_record_error_is_error() {
        let error: Box<dyn std::error::Error> = Box::new(RecordError::ChannelFull);
        assert!(error.to_string().contains("Channel full"));
    }

    #[test]
    fn test_record_error_equality() {
        assert_eq!(RecordError::ChannelFull, RecordError::ChannelFull);
        assert_eq!(RecordError::ChannelClosed, RecordError::ChannelClosed);
        assert_ne!(RecordError::ChannelFull, RecordError::ChannelClosed);
    }

    #[tokio::test]
    async fn test_record_price_async() {
        let temp_dir = TempDir::new().unwrap();
        let config = RecorderConfig {
            output_dir: temp_dir.path().to_path_buf(),
            rotation_interval_secs: 3600,
            buffer_size: 1,
            flush_interval_secs: 1,
        };

        let recorder = DataRecorder::new(config);

        let tick = PriceTick {
            symbol: "BTCUSDT".to_string(),
            price: dec!(42500.00),
            timestamp: Utc::now(),
            exchange_ts: Utc::now(),
        };

        recorder.record_price_async(tick).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let stats = recorder.stats();
        assert_eq!(stats.price_ticks_received, 1);
    }

    #[tokio::test]
    async fn test_record_orderbook_async() {
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

        recorder.record_orderbook_async(book).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let stats = recorder.stats();
        assert_eq!(stats.orderbook_updates_received, 1);
    }

    #[test]
    fn test_recorder_config_clone() {
        let config = RecorderConfig::default();
        let cloned = config.clone();
        assert_eq!(config.buffer_size, cloned.buffer_size);
        assert_eq!(config.output_dir, cloned.output_dir);
    }

    #[test]
    fn test_recorder_stats_clone() {
        let stats = RecorderStats {
            price_ticks_received: 100,
            price_ticks_written: 90,
            orderbook_updates_received: 50,
            orderbook_updates_written: 45,
            files_written: 5,
            channel_drops: 2,
        };
        let cloned = stats.clone();
        assert_eq!(stats.price_ticks_received, cloned.price_ticks_received);
        assert_eq!(stats.channel_drops, cloned.channel_drops);
    }

    #[tokio::test]
    async fn test_multiple_price_ticks() {
        let temp_dir = TempDir::new().unwrap();
        let config = RecorderConfig {
            output_dir: temp_dir.path().to_path_buf(),
            rotation_interval_secs: 3600,
            buffer_size: 10,
            flush_interval_secs: 1,
        };

        let recorder = DataRecorder::new(config);

        // Record multiple ticks
        for i in 0..5 {
            let tick = PriceTick {
                symbol: "BTCUSDT".to_string(),
                price: dec!(42500.00) + rust_decimal::Decimal::from(i),
                timestamp: Utc::now(),
                exchange_ts: Utc::now(),
            };
            recorder.record_price(tick).unwrap();
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let stats = recorder.stats();
        assert_eq!(stats.price_ticks_received, 5);
    }

    #[tokio::test]
    async fn test_atomic_recorder_stats_all_fields() {
        let stats = AtomicRecorderStats::default();

        stats.price_ticks_received.fetch_add(10, Ordering::Relaxed);
        stats.price_ticks_written.fetch_add(8, Ordering::Relaxed);
        stats
            .orderbook_updates_received
            .fetch_add(5, Ordering::Relaxed);
        stats
            .orderbook_updates_written
            .fetch_add(4, Ordering::Relaxed);
        stats.files_written.fetch_add(2, Ordering::Relaxed);
        stats.channel_drops.fetch_add(1, Ordering::Relaxed);

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.price_ticks_received, 10);
        assert_eq!(snapshot.price_ticks_written, 8);
        assert_eq!(snapshot.orderbook_updates_received, 5);
        assert_eq!(snapshot.orderbook_updates_written, 4);
        assert_eq!(snapshot.files_written, 2);
        assert_eq!(snapshot.channel_drops, 1);
    }
}
