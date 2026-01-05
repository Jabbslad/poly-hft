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
    #[serde(default)]
    pub momentum: MomentumConfig,
    #[serde(default)]
    pub lag: LagConfig,
    #[serde(default)]
    pub sizing: SizingConfig,
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

/// Momentum detection configuration
#[derive(Debug, Clone, Deserialize)]
pub struct MomentumConfig {
    /// Enable momentum detection
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Window duration for momentum calculation (seconds)
    #[serde(default = "default_momentum_window")]
    pub window_seconds: u64,

    /// Minimum move percentage to trigger momentum signal
    #[serde(default = "default_min_move_pct")]
    pub min_move_pct: Decimal,

    /// Maximum move percentage (reject extreme moves as data errors)
    #[serde(default = "default_max_move_pct")]
    pub max_move_pct: Decimal,

    /// Confirmation period: move must persist for this duration (seconds)
    #[serde(default = "default_confirmation_seconds")]
    pub confirmation_seconds: u64,
}

fn default_true() -> bool {
    true
}
fn default_momentum_window() -> u64 {
    120
}
fn default_min_move_pct() -> Decimal {
    Decimal::new(7, 3) // 0.007 = 0.7%
}
fn default_max_move_pct() -> Decimal {
    Decimal::new(5, 2) // 0.05 = 5%
}
fn default_confirmation_seconds() -> u64 {
    30
}

impl Default for MomentumConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            window_seconds: 120,
            min_move_pct: Decimal::new(7, 3), // 0.7%
            max_move_pct: Decimal::new(5, 2), // 5%
            confirmation_seconds: 30,
        }
    }
}

/// Lag detection configuration
#[derive(Debug, Clone, Deserialize)]
pub struct LagConfig {
    /// Minimum lag in cents to generate signal
    #[serde(default = "default_min_lag_cents")]
    pub min_lag_cents: Decimal,

    /// Don't buy YES if price already above this (momentum already priced in)
    #[serde(default = "default_max_yes_for_up")]
    pub max_yes_for_up: Decimal,

    /// Don't buy NO if YES price already below this (momentum already priced in)
    #[serde(default = "default_min_yes_for_down")]
    pub min_yes_for_down: Decimal,

    /// Minimum seconds after market open to trade
    #[serde(default = "default_min_seconds_after_open")]
    pub min_seconds_after_open: u64,

    /// Maximum seconds before market close to trade
    #[serde(default = "default_max_seconds_before_close")]
    pub max_seconds_before_close: u64,
}

fn default_min_lag_cents() -> Decimal {
    Decimal::new(10, 2) // 0.10
}
fn default_max_yes_for_up() -> Decimal {
    Decimal::new(60, 2) // 0.60
}
fn default_min_yes_for_down() -> Decimal {
    Decimal::new(40, 2) // 0.40
}
fn default_min_seconds_after_open() -> u64 {
    60 // 1 minute after open
}
fn default_max_seconds_before_close() -> u64 {
    120 // 2 minutes before close
}

impl Default for LagConfig {
    fn default() -> Self {
        Self {
            min_lag_cents: Decimal::new(10, 2),
            max_yes_for_up: Decimal::new(60, 2),
            min_yes_for_down: Decimal::new(40, 2),
            min_seconds_after_open: 60,
            max_seconds_before_close: 120,
        }
    }
}

/// Position sizing configuration
#[derive(Debug, Clone, Deserialize)]
pub struct SizingConfig {
    /// Sizing mode: "fixed" or "kelly"
    #[serde(default = "default_sizing_mode")]
    pub mode: SizingMode,

    /// Fixed percentage of capital per trade (for fixed mode)
    #[serde(default = "default_fixed_pct")]
    pub fixed_pct: Decimal,

    /// Maximum percentage of capital per trade
    #[serde(default = "default_max_pct")]
    pub max_pct: Decimal,
}

/// Sizing mode for position sizing
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SizingMode {
    #[default]
    Fixed,
    Kelly,
}

fn default_sizing_mode() -> SizingMode {
    SizingMode::Fixed
}
fn default_fixed_pct() -> Decimal {
    Decimal::new(10, 2) // 0.10 = 10%
}
fn default_max_pct() -> Decimal {
    Decimal::new(20, 2) // 0.20 = 20%
}

impl Default for SizingConfig {
    fn default() -> Self {
        Self {
            mode: SizingMode::Fixed,
            fixed_pct: Decimal::new(10, 2),
            max_pct: Decimal::new(20, 2),
        }
    }
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
    use rust_decimal_macros::dec;

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

    #[test]
    fn test_execution_mode_live() {
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
            mode = "live"
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
        assert_eq!(config.execution.mode, ExecutionMode::Live);
        assert!(config.telemetry.otlp_endpoint.is_none());
    }

    #[test]
    fn test_feed_config() {
        let config = FeedConfig {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
        };
        assert_eq!(config.exchange, "binance");
        assert_eq!(config.symbol, "BTCUSDT");
    }

    #[test]
    fn test_market_config() {
        let config = MarketConfig {
            asset: "BTC".to_string(),
            interval: "15m".to_string(),
            refresh_interval_secs: 30,
        };
        assert_eq!(config.asset, "BTC");
        assert_eq!(config.refresh_interval_secs, 30);
    }

    #[test]
    fn test_signal_config() {
        let config = SignalConfig {
            min_edge_threshold: dec!(0.005),
            max_edge_threshold: dec!(0.10),
        };
        assert_eq!(config.min_edge_threshold, dec!(0.005));
    }

    #[test]
    fn test_risk_config() {
        let config = RiskConfig {
            kelly_fraction: dec!(0.25),
            max_position_pct: dec!(0.01),
            max_concurrent_positions: 3,
            initial_bankroll: dec!(500),
        };
        assert_eq!(config.kelly_fraction, dec!(0.25));
    }

    #[test]
    fn test_config_load_nonexistent() {
        let result = Config::load("/nonexistent/path/config.toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_execution_mode_equality() {
        assert_eq!(ExecutionMode::Paper, ExecutionMode::Paper);
        assert_eq!(ExecutionMode::Live, ExecutionMode::Live);
        assert_ne!(ExecutionMode::Paper, ExecutionMode::Live);
    }

    #[test]
    fn test_config_clone() {
        let config = FeedConfig {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
        };
        let cloned = config.clone();
        assert_eq!(config.exchange, cloned.exchange);
    }
}
