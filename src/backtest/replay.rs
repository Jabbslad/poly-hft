//! Event-driven replay from Parquet files

use crate::feed::PriceTick;
use crate::market::Market;
use crate::orderbook::OrderBook;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Backtest event types
#[derive(Debug, Clone)]
pub enum BacktestEvent {
    /// Price tick from exchange
    PriceTick(PriceTick),
    /// Order book update
    OrderBookUpdate(OrderBook),
    /// New market opened
    MarketOpen(Market),
    /// Market closed/settled
    MarketClose(Market),
}

/// Merges multiple data sources and yields events in timestamp order
#[allow(dead_code)]
pub struct EventStream {
    data_dir: PathBuf,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
}

impl EventStream {
    /// Create a new event stream from data directory
    pub fn new(
        data_dir: PathBuf,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            data_dir,
            start_time,
            end_time,
        }
    }

    /// Get next event in timestamp order
    fn next_event(&mut self) -> Option<(DateTime<Utc>, BacktestEvent)> {
        // TODO: Implement Parquet reading and event merging
        None
    }
}

impl Iterator for EventStream {
    type Item = (DateTime<Utc>, BacktestEvent);

    fn next(&mut self) -> Option<Self::Item> {
        self.next_event()
    }
}
