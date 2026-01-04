//! Data capture module
//!
//! Stores tick data to Parquet for backtesting

mod parquet;
mod recorder;

pub use parquet::ParquetWriter;
pub use recorder::DataRecorder;
