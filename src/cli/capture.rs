//! Capture command implementation

use crate::data::{DataRecorder, RecorderConfig};
use crate::feed::{BinanceFeed, PriceFeed};
use crate::telemetry::{record_latency, record_price_tick, LatencyMetric};
use chrono::Utc;
use clap::Args;
use std::path::PathBuf;
use std::time::Duration;
use tokio::signal;

#[derive(Args, Debug)]
pub struct CaptureArgs {
    /// Output directory for captured data
    #[arg(short, long, default_value = "./data")]
    pub output: PathBuf,

    /// Trading symbol to capture
    #[arg(short, long, default_value = "btcusdt")]
    pub symbol: String,

    /// Buffer size before flushing to disk
    #[arg(long, default_value = "1000")]
    pub buffer_size: usize,

    /// Flush interval in seconds
    #[arg(long, default_value = "60")]
    pub flush_interval: u64,

    /// File rotation interval in seconds (default: 1 hour)
    #[arg(long, default_value = "3600")]
    pub rotation_interval: u64,

    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

impl CaptureArgs {
    pub async fn execute(&self) -> anyhow::Result<()> {
        tracing::info!(
            output = ?self.output,
            symbol = %self.symbol,
            "Starting data capture..."
        );

        // Create data recorder
        let recorder_config = RecorderConfig {
            output_dir: self.output.clone(),
            rotation_interval_secs: self.rotation_interval,
            buffer_size: self.buffer_size,
            flush_interval_secs: self.flush_interval,
        };
        let recorder = DataRecorder::new(recorder_config);

        // Create Binance feed
        let feed = BinanceFeed::new(&self.symbol);
        let mut rx = feed.subscribe().await?;

        tracing::info!("Connected to Binance WebSocket, capturing data...");
        println!(
            "Capturing {} data to {:?}",
            self.symbol.to_uppercase(),
            self.output
        );
        println!("Press Ctrl+C to stop");

        let mut tick_count: u64 = 0;
        let start_time = Utc::now();

        loop {
            tokio::select! {
                tick_result = rx.recv() => {
                    match tick_result {
                        Some(tick) => {
                            // Record latency
                            let latency = Utc::now() - tick.exchange_ts;
                            if let Ok(latency_std) = latency.to_std() {
                                record_latency(LatencyMetric::PriceFeed, latency_std);
                            }

                            // Record to metrics
                            record_price_tick();

                            // Record to Parquet - non-blocking!
                            if let Err(e) = recorder.record_price(tick.clone()) {
                                tracing::warn!(error = %e, "Failed to record price tick");
                            }

                            tick_count += 1;

                            #[allow(clippy::manual_is_multiple_of)]
                            if self.verbose || tick_count % 1000 == 0 {
                                let elapsed = (Utc::now() - start_time).num_seconds();
                                let rate = if elapsed > 0 { tick_count / elapsed as u64 } else { 0 };
                                tracing::info!(
                                    price = %tick.price,
                                    ticks = tick_count,
                                    rate = rate,
                                    latency_ms = latency.num_milliseconds(),
                                    "Received tick"
                                );
                            }
                        }
                        None => {
                            tracing::warn!("Feed disconnected, waiting for reconnection...");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }

                _ = signal::ctrl_c() => {
                    tracing::info!("Received shutdown signal");
                    break;
                }
            }
        }

        // Print final stats
        let stats = recorder.stats();
        let elapsed = (Utc::now() - start_time).num_seconds();

        println!("\nCapture Summary:");
        println!("  Duration: {}s", elapsed);
        println!("  Price ticks received: {}", stats.price_ticks_received);
        println!("  Price ticks written: {}", stats.price_ticks_written);
        println!("  Files written: {}", stats.files_written);
        println!("  Channel drops: {}", stats.channel_drops);
        println!("  Output directory: {:?}", self.output);

        Ok(())
    }
}
