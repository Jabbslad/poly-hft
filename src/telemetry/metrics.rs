//! Prometheus metrics

use std::time::Duration;

/// Latency metric types
#[derive(Debug, Clone, Copy)]
pub enum LatencyMetric {
    /// Binance feed latency
    PriceFeed,
    /// Polymarket order book latency
    OrderBook,
    /// Signal generation latency
    SignalGeneration,
    /// Order submission latency
    OrderSubmission,
}

/// Gauge metric types
#[derive(Debug, Clone, Copy)]
pub enum GaugeMetric {
    /// Current equity
    Equity,
    /// Unrealized P&L
    UnrealizedPnl,
    /// Realized P&L
    RealizedPnl,
    /// Open position count
    OpenPositions,
    /// Total exposure
    TotalExposure,
    /// Current drawdown percentage
    DrawdownPct,
    /// Daily P&L
    DailyPnl,
    /// Current volatility estimate
    CurrentVolatility,
    /// Active market count
    ActiveMarkets,
}

/// Record a latency measurement
pub fn record_latency(metric: LatencyMetric, duration: Duration) {
    let metric_name = match metric {
        LatencyMetric::PriceFeed => "polyhft_price_feed_latency_ms",
        LatencyMetric::OrderBook => "polyhft_orderbook_update_latency_ms",
        LatencyMetric::SignalGeneration => "polyhft_signal_generation_latency_ms",
        LatencyMetric::OrderSubmission => "polyhft_order_submission_latency_ms",
    };

    // TODO: Record to Prometheus histogram
    tracing::debug!(
        metric = metric_name,
        value_ms = duration.as_millis(),
        "Recording latency"
    );
}

/// Set a gauge value
pub fn set_gauge(metric: GaugeMetric, value: f64) {
    let metric_name = match metric {
        GaugeMetric::Equity => "polyhft_equity_usd",
        GaugeMetric::UnrealizedPnl => "polyhft_unrealized_pnl_usd",
        GaugeMetric::RealizedPnl => "polyhft_realized_pnl_usd",
        GaugeMetric::OpenPositions => "polyhft_open_positions",
        GaugeMetric::TotalExposure => "polyhft_total_exposure_usd",
        GaugeMetric::DrawdownPct => "polyhft_drawdown_pct",
        GaugeMetric::DailyPnl => "polyhft_daily_pnl_usd",
        GaugeMetric::CurrentVolatility => "polyhft_current_volatility",
        GaugeMetric::ActiveMarkets => "polyhft_active_markets",
    };

    // TODO: Set Prometheus gauge
    tracing::debug!(metric = metric_name, value = value, "Setting gauge");
}
