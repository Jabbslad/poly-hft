//! Configuration types for poly-hft

use rust_decimal::Decimal;
use serde::Deserialize;
use std::path::PathBuf;

/// Root configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub feed: FeedConfig,
    pub market: MarketConfig,
    pub model: ModelConfig,
    pub signal: SignalConfig,
    pub risk: RiskConfig,
    pub execution: ExecutionConfig,
    pub data: DataConfig,
    pub telemetry: TelemetryConfig,
}

/// Price feed configuration
#[derive(Debug, Clone, Deserialize)]
pub struct FeedConfig {
    pub exchange: String,
    pub symbol: String,
}

/// Market discovery configuration
#[derive(Debug, Clone, Deserialize)]
pub struct MarketConfig {
    pub asset: String,
    pub interval: String,
    pub refresh_interval_secs: u64,
}

/// Fair value model configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    pub volatility_window_minutes: u64,
    pub min_time_to_expiry_secs: u64,
}

/// Signal generation configuration
#[derive(Debug, Clone, Deserialize)]
pub struct SignalConfig {
    pub min_edge_threshold: Decimal,
    pub max_edge_threshold: Decimal,
}

/// Risk management configuration
#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    pub kelly_fraction: Decimal,
    pub max_position_pct: Decimal,
    pub max_concurrent_positions: usize,
    pub initial_bankroll: Decimal,
}

/// Execution engine configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ExecutionConfig {
    pub mode: ExecutionMode,
    pub slippage_estimate: Decimal,
}

/// Execution mode: paper trading or live
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    Paper,
    Live,
}

/// Data capture configuration
#[derive(Debug, Clone, Deserialize)]
pub struct DataConfig {
    pub capture_enabled: bool,
    pub output_dir: PathBuf,
    pub rotation_interval: String,
}

/// Telemetry configuration
#[derive(Debug, Clone, Deserialize)]
pub struct TelemetryConfig {
    pub metrics_port: u16,
    pub log_level: String,
    pub otlp_endpoint: Option<String>,
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialize() {
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
            otlp_endpoint = "http://localhost:4317"
        "#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.feed.exchange, "binance");
        assert_eq!(config.risk.max_concurrent_positions, 3);
        assert_eq!(config.execution.mode, ExecutionMode::Paper);
    }
}
