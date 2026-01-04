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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::path::PathBuf;

    #[test]
    fn test_event_stream_creation() {
        let stream = EventStream::new(PathBuf::from("./data"), None, None);
        assert_eq!(stream.data_dir, PathBuf::from("./data"));
        assert!(stream.start_time.is_none());
        assert!(stream.end_time.is_none());
    }

    #[test]
    fn test_event_stream_with_time_bounds() {
        let start = Utc::now();
        let end = Utc::now();
        let stream = EventStream::new(PathBuf::from("./data"), Some(start), Some(end));
        assert!(stream.start_time.is_some());
        assert!(stream.end_time.is_some());
    }

    #[test]
    fn test_event_stream_iterator_empty() {
        let mut stream = EventStream::new(PathBuf::from("./nonexistent"), None, None);
        // Should return None since no data is loaded
        assert!(stream.next().is_none());
    }

    #[test]
    fn test_backtest_event_price_tick() {
        let tick = PriceTick {
            symbol: "BTCUSDT".to_string(),
            price: dec!(42000),
            timestamp: Utc::now(),
            exchange_ts: Utc::now(),
        };

        let event = BacktestEvent::PriceTick(tick.clone());
        assert!(matches!(event, BacktestEvent::PriceTick(_)));
    }

    #[test]
    fn test_backtest_event_orderbook() {
        use crate::orderbook::{OrderBook, PriceLevel};

        let book = OrderBook {
            token_id: "yes-token".to_string(),
            bids: vec![PriceLevel {
                price: dec!(0.50),
                size: dec!(100),
            }],
            asks: vec![PriceLevel {
                price: dec!(0.52),
                size: dec!(100),
            }],
            updated_at: Utc::now(),
        };

        let event = BacktestEvent::OrderBookUpdate(book);
        assert!(matches!(event, BacktestEvent::OrderBookUpdate(_)));
    }

    #[test]
    fn test_backtest_event_market_open() {
        let market = Market {
            condition_id: "cond".to_string(),
            yes_token_id: "yes".to_string(),
            no_token_id: "no".to_string(),
            open_price: dec!(100000),
            open_time: Utc::now(),
            close_time: Utc::now(),
        };

        let event = BacktestEvent::MarketOpen(market);
        assert!(matches!(event, BacktestEvent::MarketOpen(_)));
    }

    #[test]
    fn test_backtest_event_market_close() {
        let market = Market {
            condition_id: "cond".to_string(),
            yes_token_id: "yes".to_string(),
            no_token_id: "no".to_string(),
            open_price: dec!(100000),
            open_time: Utc::now(),
            close_time: Utc::now(),
        };

        let event = BacktestEvent::MarketClose(market);
        assert!(matches!(event, BacktestEvent::MarketClose(_)));
    }

    #[test]
    fn test_backtest_event_clone() {
        let tick = PriceTick {
            symbol: "BTCUSDT".to_string(),
            price: dec!(42000),
            timestamp: Utc::now(),
            exchange_ts: Utc::now(),
        };

        let event = BacktestEvent::PriceTick(tick);
        let cloned = event.clone();
        assert!(matches!(cloned, BacktestEvent::PriceTick(_)));
    }
}
