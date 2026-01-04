//! Structured logging setup

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Log output format
#[derive(Debug, Clone, Copy)]
pub enum LogFormat {
    /// Human-readable format
    Pretty,
    /// JSON format for log aggregation
    Json,
}

/// Initialize logging with the given level
pub fn init_logging(level: &str) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .try_init()
        .map_err(|e| anyhow::anyhow!("Failed to init logging: {}", e))?;

    Ok(())
}
