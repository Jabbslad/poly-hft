//! Telemetry module
//!
//! Metrics, logging, and distributed tracing

mod logging;
mod metrics;
mod tracing_setup;

pub use logging::{init_logging, LogFormat};
pub use metrics::{record_latency, set_gauge, GaugeMetric, LatencyMetric};
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

    // TODO: Start metrics server on config.metrics_port

    Ok(TelemetryGuard { _priv: () })
}
