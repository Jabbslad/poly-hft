//! Data capture module
//!
//! Stores tick data to Parquet for backtesting

mod parquet;
mod recorder;

pub use parquet::{
    orderbook_schema, price_tick_schema, signal_schema, OrderBookRecord, ParquetReader,
    ParquetWriter, PriceTickRecord, SignalRecord,
};
pub use recorder::{AtomicRecorderStats, DataRecorder, RecordError, RecorderConfig, RecorderStats};
