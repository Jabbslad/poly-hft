//! Integration tests for market discovery module

use poly_hft::market::{GammaClient, MarketTracker, MarketTrackerImpl};

#[tokio::test]
async fn test_gamma_client_creation() {
    let client = GammaClient::new();
    let tracker = MarketTrackerImpl::new(client);
    let markets = tracker.get_active_markets().await.unwrap();
    assert!(markets.is_empty()); // Empty until implemented
}
