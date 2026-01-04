//! End-to-end integration tests

use poly_hft::config::Config;

#[test]
fn test_config_example_exists() {
    // This test ensures the example config can be loaded
    // In a real test, we'd load from config.toml.example
    let toml = r#"
        [feed]
        exchange = "binance"
        symbol = "BTCUSDT"

        [market]
        asset = "BTC"
        interval = "15m"
        refresh_interval_secs = 30

        [model]
        volatility_window_minutes = 30
        min_time_to_expiry_secs = 60

        [signal]
        min_edge_threshold = 0.005
        max_edge_threshold = 0.10

        [risk]
        kelly_fraction = 0.25
        max_position_pct = 0.01
        max_concurrent_positions = 3
        initial_bankroll = 500.0

        [execution]
        mode = "paper"
        slippage_estimate = 0.001

        [data]
        capture_enabled = true
        output_dir = "./data"
        rotation_interval = "1h"

        [telemetry]
        metrics_port = 9090
        log_level = "info"
    "#;

    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.feed.symbol, "BTCUSDT");
}
