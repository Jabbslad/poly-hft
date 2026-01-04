//! Integration tests for price feed module

use poly_hft::feed::{BinanceFeed, PriceFeed};

#[tokio::test]
async fn test_binance_feed_subscribe() {
    let feed = BinanceFeed::new("btcusdt");
    let result = feed.subscribe().await;
    assert!(result.is_ok());
}
