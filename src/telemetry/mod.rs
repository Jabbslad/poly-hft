//! Telemetry module
//!
//! Metrics, logging, and distributed tracing

mod logging;
mod metrics;
mod tracing_setup;

pub use logging::{init_logging, LogFormat};
pub use metrics::{
    increment_counter, increment_counter_simple, init_metrics_server, record_error, record_fill,
    record_latency, record_order, record_orderbook_update, record_price_tick, record_signal,
    record_ws_reconnect, set_gauge, CounterMetric, GaugeMetric, LatencyMetric,
};
pub use tracing_setup::init_tracing;

use crate::config::TelemetryConfig;

/// Guard that cleans up telemetry on drop
pub struct TelemetryGuard {
    _priv: (),
}

/// Initialize all telemetry subsystems
pub fn init_telemetry(config: &TelemetryConfig) -> anyhow::Result<TelemetryGuard> {
    init_logging(&config.log_level)?;

    if let Some(ref endpoint) = config.otlp_endpoint {
        init_tracing(endpoint)?;
    }

    // Start metrics server
    init_metrics_server(config.metrics_port)?;

    Ok(TelemetryGuard { _priv: () })
}
