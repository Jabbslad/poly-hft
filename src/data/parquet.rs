//! Parquet file writer with rotation

use arrow::array::{ArrayRef, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Duration, Utc};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use rust_decimal::Decimal;
use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::Arc;

/// Price tick schema fields
pub fn price_tick_schema() -> Schema {
    Schema::new(vec![
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("symbol", DataType::Utf8, false),
        Field::new("price", DataType::Utf8, false), // Store as string for Decimal precision
        Field::new(
            "exchange_ts",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
    ])
}

/// Order book schema fields (top 5 levels)
pub fn orderbook_schema() -> Schema {
    let mut fields = vec![
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("token_id", DataType::Utf8, false),
    ];

    // Add bid/ask price and size for 5 levels
    for i in 0..5 {
        fields.push(Field::new(format!("bid_price_{}", i), DataType::Utf8, true));
        fields.push(Field::new(format!("bid_size_{}", i), DataType::Utf8, true));
        fields.push(Field::new(format!("ask_price_{}", i), DataType::Utf8, true));
        fields.push(Field::new(format!("ask_size_{}", i), DataType::Utf8, true));
    }

    Schema::new(fields)
}

/// Parquet file writer with time-based rotation
#[derive(Clone)]
pub struct ParquetWriter {
    output_dir: PathBuf,
    rotation_interval: Duration,
    current_file_start: Option<DateTime<Utc>>,
}

impl ParquetWriter {
    /// Create a new Parquet writer
    pub fn new(output_dir: PathBuf, rotation_interval_secs: u64) -> Self {
        Self {
            output_dir,
            rotation_interval: Duration::seconds(rotation_interval_secs as i64),
            current_file_start: None,
        }
    }

    /// Ensure output directory exists
    fn ensure_dir(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.output_dir)?;
        Ok(())
    }

    /// Check if rotation is needed based on current time
    pub fn needs_rotation(&self, now: DateTime<Utc>) -> bool {
        match self.current_file_start {
            None => true,
            Some(start) => now - start >= self.rotation_interval,
        }
    }

    /// Generate file path for a given timestamp and prefix
    pub fn file_path(&self, prefix: &str, timestamp: DateTime<Utc>) -> PathBuf {
        let filename = format!("{}_{}.parquet", prefix, timestamp.format("%Y%m%d_%H%M%S"));
        self.output_dir.join(filename)
    }

    /// Get current output file path (for compatibility)
    pub fn current_path(&self, prefix: &str) -> PathBuf {
        let now = Utc::now();
        self.file_path(prefix, now)
    }

    /// Update rotation timestamp
    pub fn mark_rotation(&mut self, timestamp: DateTime<Utc>) {
        self.current_file_start = Some(timestamp);
    }

    /// Write price ticks to a Parquet file (blocking - use write_price_ticks_async for async)
    pub fn write_price_ticks(
        &self,
        path: &PathBuf,
        ticks: &[PriceTickRecord],
    ) -> anyhow::Result<()> {
        if ticks.is_empty() {
            return Ok(());
        }

        self.ensure_dir()?;

        let schema = Arc::new(price_tick_schema());
        let file = File::create(path)?;

        let props = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build();

        let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props))?;

        // Build arrays
        let timestamps: Vec<i64> = ticks
            .iter()
            .map(|t| t.timestamp.timestamp_micros())
            .collect();
        let symbols: Vec<&str> = ticks.iter().map(|t| t.symbol.as_ref()).collect();
        let prices: Vec<String> = ticks.iter().map(|t| t.price.to_string()).collect();
        let exchange_ts: Vec<i64> = ticks
            .iter()
            .map(|t| t.exchange_ts.timestamp_micros())
            .collect();

        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(TimestampMicrosecondArray::from(timestamps).with_timezone("UTC"))
                    as ArrayRef,
                Arc::new(StringArray::from(symbols)) as ArrayRef,
                Arc::new(StringArray::from(
                    prices.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                )) as ArrayRef,
                Arc::new(TimestampMicrosecondArray::from(exchange_ts).with_timezone("UTC"))
                    as ArrayRef,
            ],
        )?;

        writer.write(&batch)?;
        writer.close()?;

        tracing::debug!(path = ?path, count = ticks.len(), "Wrote price ticks to Parquet");

        Ok(())
    }

    /// Write price ticks asynchronously using spawn_blocking
    pub async fn write_price_ticks_async(
        &self,
        path: PathBuf,
        ticks: Vec<PriceTickRecord>,
    ) -> anyhow::Result<()> {
        if ticks.is_empty() {
            return Ok(());
        }

        let writer = self.clone();
        tokio::task::spawn_blocking(move || writer.write_price_ticks(&path, &ticks))
            .await
            .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }

    /// Write order book snapshots to a Parquet file (blocking)
    pub fn write_orderbook_snapshots(
        &self,
        path: &PathBuf,
        snapshots: &[OrderBookRecord],
    ) -> anyhow::Result<()> {
        if snapshots.is_empty() {
            return Ok(());
        }

        self.ensure_dir()?;

        let schema = Arc::new(orderbook_schema());
        let file = File::create(path)?;

        let props = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build();

        let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props))?;

        // Build arrays
        let timestamps: Vec<i64> = snapshots
            .iter()
            .map(|s| s.timestamp.timestamp_micros())
            .collect();
        let token_ids: Vec<&str> = snapshots.iter().map(|s| s.token_id.as_ref()).collect();

        let mut columns: Vec<ArrayRef> = vec![
            Arc::new(TimestampMicrosecondArray::from(timestamps).with_timezone("UTC")),
            Arc::new(StringArray::from(token_ids)),
        ];

        // Add bid/ask levels
        for i in 0..5 {
            let bid_prices: Vec<Option<String>> = snapshots
                .iter()
                .map(|s| s.bids.get(i).map(|(p, _)| p.to_string()))
                .collect();
            let bid_sizes: Vec<Option<String>> = snapshots
                .iter()
                .map(|s| s.bids.get(i).map(|(_, s)| s.to_string()))
                .collect();
            let ask_prices: Vec<Option<String>> = snapshots
                .iter()
                .map(|s| s.asks.get(i).map(|(p, _)| p.to_string()))
                .collect();
            let ask_sizes: Vec<Option<String>> = snapshots
                .iter()
                .map(|s| s.asks.get(i).map(|(_, s)| s.to_string()))
                .collect();

            columns.push(Arc::new(StringArray::from(bid_prices)));
            columns.push(Arc::new(StringArray::from(bid_sizes)));
            columns.push(Arc::new(StringArray::from(ask_prices)));
            columns.push(Arc::new(StringArray::from(ask_sizes)));
        }

        let batch = RecordBatch::try_new(schema, columns)?;

        writer.write(&batch)?;
        writer.close()?;

        tracing::debug!(path = ?path, count = snapshots.len(), "Wrote orderbook snapshots to Parquet");

        Ok(())
    }

    /// Write order book snapshots asynchronously using spawn_blocking
    pub async fn write_orderbook_snapshots_async(
        &self,
        path: PathBuf,
        snapshots: Vec<OrderBookRecord>,
    ) -> anyhow::Result<()> {
        if snapshots.is_empty() {
            return Ok(());
        }

        let writer = self.clone();
        tokio::task::spawn_blocking(move || writer.write_orderbook_snapshots(&path, &snapshots))
            .await
            .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }
}

/// Record type for price ticks (for writing)
/// Uses Arc<str> for symbol to reduce allocations on hot path
#[derive(Debug, Clone)]
pub struct PriceTickRecord {
    pub timestamp: DateTime<Utc>,
    pub symbol: Arc<str>,
    pub price: Decimal,
    pub exchange_ts: DateTime<Utc>,
}

impl PriceTickRecord {
    /// Create a new price tick record
    pub fn new(
        timestamp: DateTime<Utc>,
        symbol: Arc<str>,
        price: Decimal,
        exchange_ts: DateTime<Utc>,
    ) -> Self {
        Self {
            timestamp,
            symbol,
            price,
            exchange_ts,
        }
    }
}

/// Record type for order book snapshots (for writing)
/// Uses Arc<str> for token_id to reduce allocations
#[derive(Debug, Clone)]
pub struct OrderBookRecord {
    pub timestamp: DateTime<Utc>,
    pub token_id: Arc<str>,
    pub bids: Vec<(Decimal, Decimal)>, // (price, size)
    pub asks: Vec<(Decimal, Decimal)>,
}

/// Reader for Parquet files
pub struct ParquetReader {
    path: PathBuf,
}

impl ParquetReader {
    /// Create a new reader for a Parquet file
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Read price ticks from a Parquet file
    pub fn read_price_ticks(&self) -> anyhow::Result<Vec<PriceTickRecord>> {
        use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
        use std::str::FromStr;

        let file = File::open(&self.path)?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
        let reader = builder.build()?;

        let mut ticks = Vec::new();

        for batch_result in reader {
            let batch = batch_result?;

            let timestamps = batch
                .column(0)
                .as_any()
                .downcast_ref::<TimestampMicrosecondArray>()
                .ok_or_else(|| anyhow::anyhow!("Invalid timestamp column"))?;

            let symbols = batch
                .column(1)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| anyhow::anyhow!("Invalid symbol column"))?;

            let prices = batch
                .column(2)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| anyhow::anyhow!("Invalid price column"))?;

            let exchange_timestamps = batch
                .column(3)
                .as_any()
                .downcast_ref::<TimestampMicrosecondArray>()
                .ok_or_else(|| anyhow::anyhow!("Invalid exchange_ts column"))?;

            for i in 0..batch.num_rows() {
                let timestamp = DateTime::from_timestamp_micros(timestamps.value(i))
                    .ok_or_else(|| anyhow::anyhow!("Invalid timestamp"))?;
                let exchange_ts = DateTime::from_timestamp_micros(exchange_timestamps.value(i))
                    .ok_or_else(|| anyhow::anyhow!("Invalid exchange_ts"))?;

                ticks.push(PriceTickRecord {
                    timestamp,
                    symbol: Arc::from(symbols.value(i)),
                    price: Decimal::from_str(prices.value(i))?,
                    exchange_ts,
                });
            }
        }

        Ok(ticks)
    }

    /// Read price ticks asynchronously
    pub async fn read_price_ticks_async(&self) -> anyhow::Result<Vec<PriceTickRecord>> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let reader = ParquetReader::new(path);
            reader.read_price_ticks()
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }

    /// Get the file path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

/// Signal record for writing to Parquet
#[derive(Debug, Clone)]
pub struct SignalRecord {
    pub timestamp: DateTime<Utc>,
    pub market_id: Arc<str>,
    pub side: Arc<str>,
    pub fair_value: Decimal,
    pub market_price: Decimal,
    pub edge: Decimal,
    pub action: Arc<str>,
}

/// Signal schema
pub fn signal_schema() -> Schema {
    Schema::new(vec![
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("market_id", DataType::Utf8, false),
        Field::new("side", DataType::Utf8, false),
        Field::new("fair_value", DataType::Utf8, false),
        Field::new("market_price", DataType::Utf8, false),
        Field::new("edge", DataType::Utf8, false),
        Field::new("action", DataType::Utf8, false),
    ])
}

impl ParquetWriter {
    /// Write signal records to a Parquet file
    pub fn write_signals(&self, path: &PathBuf, signals: &[SignalRecord]) -> anyhow::Result<()> {
        if signals.is_empty() {
            return Ok(());
        }

        self.ensure_dir()?;

        let schema = Arc::new(signal_schema());
        let file = File::create(path)?;

        let props = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build();

        let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props))?;

        let timestamps: Vec<i64> = signals
            .iter()
            .map(|s| s.timestamp.timestamp_micros())
            .collect();
        let market_ids: Vec<&str> = signals.iter().map(|s| s.market_id.as_ref()).collect();
        let sides: Vec<&str> = signals.iter().map(|s| s.side.as_ref()).collect();
        let fair_values: Vec<String> = signals.iter().map(|s| s.fair_value.to_string()).collect();
        let market_prices: Vec<String> =
            signals.iter().map(|s| s.market_price.to_string()).collect();
        let edges: Vec<String> = signals.iter().map(|s| s.edge.to_string()).collect();
        let actions: Vec<&str> = signals.iter().map(|s| s.action.as_ref()).collect();

        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(TimestampMicrosecondArray::from(timestamps).with_timezone("UTC"))
                    as ArrayRef,
                Arc::new(StringArray::from(market_ids)) as ArrayRef,
                Arc::new(StringArray::from(sides)) as ArrayRef,
                Arc::new(StringArray::from(
                    fair_values.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                )) as ArrayRef,
                Arc::new(StringArray::from(
                    market_prices.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                )) as ArrayRef,
                Arc::new(StringArray::from(
                    edges.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                )) as ArrayRef,
                Arc::new(StringArray::from(actions)) as ArrayRef,
            ],
        )?;

        writer.write(&batch)?;
        writer.close()?;

        tracing::debug!(path = ?path, count = signals.len(), "Wrote signals to Parquet");

        Ok(())
    }

    /// Write signals asynchronously using spawn_blocking
    pub async fn write_signals_async(
        &self,
        path: PathBuf,
        signals: Vec<SignalRecord>,
    ) -> anyhow::Result<()> {
        if signals.is_empty() {
            return Ok(());
        }

        let writer = self.clone();
        tokio::task::spawn_blocking(move || writer.write_signals(&path, &signals))
            .await
            .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use tempfile::TempDir;

    #[test]
    fn test_price_tick_schema() {
        let schema = price_tick_schema();
        assert_eq!(schema.fields().len(), 4);
        assert_eq!(schema.field(0).name(), "timestamp");
        assert_eq!(schema.field(1).name(), "symbol");
        assert_eq!(schema.field(2).name(), "price");
        assert_eq!(schema.field(3).name(), "exchange_ts");
    }

    #[test]
    fn test_orderbook_schema() {
        let schema = orderbook_schema();
        // 2 base fields + 5 levels * 4 fields each = 22 fields
        assert_eq!(schema.fields().len(), 22);
    }

    #[test]
    fn test_parquet_writer_file_path() {
        let writer = ParquetWriter::new(PathBuf::from("/data"), 3600);
        let timestamp = DateTime::parse_from_rfc3339("2025-01-04T12:30:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let path = writer.file_path("price_ticks", timestamp);
        assert_eq!(
            path,
            PathBuf::from("/data/price_ticks_20250104_123000.parquet")
        );
    }

    #[test]
    fn test_parquet_writer_needs_rotation() {
        let mut writer = ParquetWriter::new(PathBuf::from("/data"), 3600);
        let now = Utc::now();

        // Initially needs rotation
        assert!(writer.needs_rotation(now));

        // After marking, doesn't need rotation
        writer.mark_rotation(now);
        assert!(!writer.needs_rotation(now));

        // After interval passes, needs rotation again
        let future = now + Duration::hours(2);
        assert!(writer.needs_rotation(future));
    }

    #[test]
    fn test_write_and_read_price_ticks() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ParquetWriter::new(temp_dir.path().to_path_buf(), 3600);

        let now = Utc::now();
        let ticks = vec![
            PriceTickRecord {
                timestamp: now,
                symbol: Arc::from("BTCUSDT"),
                price: dec!(42500.50),
                exchange_ts: now,
            },
            PriceTickRecord {
                timestamp: now,
                symbol: Arc::from("BTCUSDT"),
                price: dec!(42501.25),
                exchange_ts: now,
            },
        ];

        let path = writer.file_path("price_ticks", now);
        writer.write_price_ticks(&path, &ticks).unwrap();

        // Read back
        let reader = ParquetReader::new(path);
        let read_ticks = reader.read_price_ticks().unwrap();

        assert_eq!(read_ticks.len(), 2);
        assert_eq!(read_ticks[0].symbol.as_ref(), "BTCUSDT");
        assert_eq!(read_ticks[0].price, dec!(42500.50));
        assert_eq!(read_ticks[1].price, dec!(42501.25));
    }

    #[test]
    fn test_write_empty_ticks() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ParquetWriter::new(temp_dir.path().to_path_buf(), 3600);

        let path = writer.file_path("price_ticks", Utc::now());
        // Should succeed without creating file
        writer.write_price_ticks(&path, &[]).unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn test_write_price_ticks_async() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ParquetWriter::new(temp_dir.path().to_path_buf(), 3600);

        let now = Utc::now();
        let ticks = vec![PriceTickRecord {
            timestamp: now,
            symbol: Arc::from("BTCUSDT"),
            price: dec!(42500.50),
            exchange_ts: now,
        }];

        let path = writer.file_path("price_ticks", now);
        writer
            .write_price_ticks_async(path.clone(), ticks)
            .await
            .unwrap();

        // Verify file was created
        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_write_empty_ticks_async() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ParquetWriter::new(temp_dir.path().to_path_buf(), 3600);

        let path = writer.file_path("price_ticks", Utc::now());
        // Should succeed without creating file
        writer
            .write_price_ticks_async(path.clone(), vec![])
            .await
            .unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_write_and_read_orderbook_snapshots() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ParquetWriter::new(temp_dir.path().to_path_buf(), 3600);

        let now = Utc::now();
        let snapshots = vec![
            OrderBookRecord {
                timestamp: now,
                token_id: Arc::from("yes-token"),
                bids: vec![(dec!(0.55), dec!(100)), (dec!(0.54), dec!(200))],
                asks: vec![(dec!(0.56), dec!(150)), (dec!(0.57), dec!(250))],
            },
            OrderBookRecord {
                timestamp: now,
                token_id: Arc::from("no-token"),
                bids: vec![(dec!(0.45), dec!(50))],
                asks: vec![(dec!(0.46), dec!(75))],
            },
        ];

        let path = writer.file_path("orderbook", now);
        writer.write_orderbook_snapshots(&path, &snapshots).unwrap();

        // Verify file was created
        assert!(path.exists());
    }

    #[test]
    fn test_write_empty_orderbook_snapshots() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ParquetWriter::new(temp_dir.path().to_path_buf(), 3600);

        let path = writer.file_path("orderbook", Utc::now());
        writer.write_orderbook_snapshots(&path, &[]).unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn test_write_orderbook_snapshots_async() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ParquetWriter::new(temp_dir.path().to_path_buf(), 3600);

        let now = Utc::now();
        let snapshots = vec![OrderBookRecord {
            timestamp: now,
            token_id: Arc::from("test-token"),
            bids: vec![(dec!(0.50), dec!(100))],
            asks: vec![(dec!(0.52), dec!(100))],
        }];

        let path = writer.file_path("orderbook", now);
        writer
            .write_orderbook_snapshots_async(path.clone(), snapshots)
            .await
            .unwrap();

        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_write_empty_orderbook_snapshots_async() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ParquetWriter::new(temp_dir.path().to_path_buf(), 3600);

        let path = writer.file_path("orderbook", Utc::now());
        writer
            .write_orderbook_snapshots_async(path.clone(), vec![])
            .await
            .unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn test_read_price_ticks_async() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ParquetWriter::new(temp_dir.path().to_path_buf(), 3600);

        let now = Utc::now();
        let ticks = vec![PriceTickRecord {
            timestamp: now,
            symbol: Arc::from("BTCUSDT"),
            price: dec!(42500.50),
            exchange_ts: now,
        }];

        let path = writer.file_path("price_ticks", now);
        writer.write_price_ticks(&path, &ticks).unwrap();

        // Read back asynchronously
        let reader = ParquetReader::new(path);
        let read_ticks = reader.read_price_ticks_async().await.unwrap();

        assert_eq!(read_ticks.len(), 1);
        assert_eq!(read_ticks[0].symbol.as_ref(), "BTCUSDT");
    }

    #[test]
    fn test_parquet_reader_path() {
        let path = PathBuf::from("/data/test.parquet");
        let reader = ParquetReader::new(path.clone());
        assert_eq!(reader.path(), &path);
    }

    #[test]
    fn test_signal_schema() {
        let schema = signal_schema();
        assert_eq!(schema.fields().len(), 7);
        assert_eq!(schema.field(0).name(), "timestamp");
        assert_eq!(schema.field(1).name(), "market_id");
        assert_eq!(schema.field(2).name(), "side");
        assert_eq!(schema.field(3).name(), "fair_value");
        assert_eq!(schema.field(4).name(), "market_price");
        assert_eq!(schema.field(5).name(), "edge");
        assert_eq!(schema.field(6).name(), "action");
    }

    #[test]
    fn test_write_signals() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ParquetWriter::new(temp_dir.path().to_path_buf(), 3600);

        let now = Utc::now();
        let signals = vec![
            SignalRecord {
                timestamp: now,
                market_id: Arc::from("market-123"),
                side: Arc::from("YES"),
                fair_value: dec!(0.55),
                market_price: dec!(0.50),
                edge: dec!(0.05),
                action: Arc::from("BUY"),
            },
            SignalRecord {
                timestamp: now,
                market_id: Arc::from("market-456"),
                side: Arc::from("NO"),
                fair_value: dec!(0.45),
                market_price: dec!(0.50),
                edge: dec!(-0.05),
                action: Arc::from("HOLD"),
            },
        ];

        let path = writer.file_path("signals", now);
        writer.write_signals(&path, &signals).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_write_empty_signals() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ParquetWriter::new(temp_dir.path().to_path_buf(), 3600);

        let path = writer.file_path("signals", Utc::now());
        writer.write_signals(&path, &[]).unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn test_write_signals_async() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ParquetWriter::new(temp_dir.path().to_path_buf(), 3600);

        let now = Utc::now();
        let signals = vec![SignalRecord {
            timestamp: now,
            market_id: Arc::from("market-123"),
            side: Arc::from("YES"),
            fair_value: dec!(0.55),
            market_price: dec!(0.50),
            edge: dec!(0.05),
            action: Arc::from("BUY"),
        }];

        let path = writer.file_path("signals", now);
        writer
            .write_signals_async(path.clone(), signals)
            .await
            .unwrap();

        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_write_empty_signals_async() {
        let temp_dir = TempDir::new().unwrap();
        let writer = ParquetWriter::new(temp_dir.path().to_path_buf(), 3600);

        let path = writer.file_path("signals", Utc::now());
        writer
            .write_signals_async(path.clone(), vec![])
            .await
            .unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_price_tick_record_new() {
        let now = Utc::now();
        let record = PriceTickRecord::new(now, Arc::from("BTCUSDT"), dec!(42500.50), now);
        assert_eq!(record.symbol.as_ref(), "BTCUSDT");
        assert_eq!(record.price, dec!(42500.50));
    }

    #[test]
    fn test_current_path() {
        let writer = ParquetWriter::new(PathBuf::from("/data"), 3600);
        let path = writer.current_path("test");
        assert!(path.to_str().unwrap().starts_with("/data/test_"));
        assert!(path.to_str().unwrap().ends_with(".parquet"));
    }

    #[test]
    fn test_orderbook_record_clone() {
        let record = OrderBookRecord {
            timestamp: Utc::now(),
            token_id: Arc::from("test"),
            bids: vec![(dec!(0.50), dec!(100))],
            asks: vec![(dec!(0.52), dec!(100))],
        };
        let cloned = record.clone();
        assert_eq!(record.token_id, cloned.token_id);
        assert_eq!(record.bids.len(), cloned.bids.len());
    }

    #[test]
    fn test_signal_record_clone() {
        let record = SignalRecord {
            timestamp: Utc::now(),
            market_id: Arc::from("market-123"),
            side: Arc::from("YES"),
            fair_value: dec!(0.55),
            market_price: dec!(0.50),
            edge: dec!(0.05),
            action: Arc::from("BUY"),
        };
        let cloned = record.clone();
        assert_eq!(record.market_id, cloned.market_id);
        assert_eq!(record.fair_value, cloned.fair_value);
    }
}
