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
    pub fn ensure_dir(&self) -> anyhow::Result<()> {
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

    /// Write price ticks to a Parquet file
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
        let symbols: Vec<&str> = ticks.iter().map(|t| t.symbol.as_str()).collect();
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

    /// Write order book snapshots to a Parquet file
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
        let token_ids: Vec<&str> = snapshots.iter().map(|s| s.token_id.as_str()).collect();

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
}

/// Record type for price ticks (for writing)
#[derive(Debug, Clone)]
pub struct PriceTickRecord {
    pub timestamp: DateTime<Utc>,
    pub symbol: String,
    pub price: Decimal,
    pub exchange_ts: DateTime<Utc>,
}

/// Record type for order book snapshots (for writing)
#[derive(Debug, Clone)]
pub struct OrderBookRecord {
    pub timestamp: DateTime<Utc>,
    pub token_id: String,
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
                    symbol: symbols.value(i).to_string(),
                    price: Decimal::from_str(prices.value(i))?,
                    exchange_ts,
                });
            }
        }

        Ok(ticks)
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
    pub market_id: String,
    pub side: String,
    pub fair_value: Decimal,
    pub market_price: Decimal,
    pub edge: Decimal,
    pub action: String,
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
        let market_ids: Vec<&str> = signals.iter().map(|s| s.market_id.as_str()).collect();
        let sides: Vec<&str> = signals.iter().map(|s| s.side.as_str()).collect();
        let fair_values: Vec<String> = signals.iter().map(|s| s.fair_value.to_string()).collect();
        let market_prices: Vec<String> =
            signals.iter().map(|s| s.market_price.to_string()).collect();
        let edges: Vec<String> = signals.iter().map(|s| s.edge.to_string()).collect();
        let actions: Vec<&str> = signals.iter().map(|s| s.action.as_str()).collect();

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
                symbol: "BTCUSDT".to_string(),
                price: dec!(42500.50),
                exchange_ts: now,
            },
            PriceTickRecord {
                timestamp: now,
                symbol: "BTCUSDT".to_string(),
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
        assert_eq!(read_ticks[0].symbol, "BTCUSDT");
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
}
