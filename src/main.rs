use clap::Parser;
use poly_hft::cli::{Cli, Commands};
use poly_hft::config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let config = Config::load(&cli.config).unwrap_or_else(|e| {
        eprintln!("Warning: Could not load config from {}: {}", cli.config, e);
        eprintln!("Using default configuration");
        // Return a default config for now
        toml::from_str(include_str!("../config.toml.example")).expect("Invalid default config")
    });

    // Initialize telemetry
    poly_hft::telemetry::init_telemetry(&config.telemetry)?;

    match cli.command {
        Commands::Run(args) => {
            tracing::info!("Starting paper trading mode");
            args.execute().await?;
        }
        Commands::Capture(args) => {
            tracing::info!("Starting data capture mode");
            args.execute().await?;
        }
        Commands::Backtest(args) => {
            tracing::info!("Starting backtest");
            args.execute().await?;
        }
        Commands::Status => {
            println!("poly-hft status");
            println!("  Mode: Paper Trading");
            println!("  Status: Not running");
        }
        Commands::Config => {
            println!("Current configuration:");
            println!("  Feed: {} {}", config.feed.exchange, config.feed.symbol);
            println!(
                "  Market: {} {}",
                config.market.asset, config.market.interval
            );
            println!("  Execution: {:?}", config.execution.mode);
            println!(
                "  Risk: Kelly={}, MaxPos={}%",
                config.risk.kelly_fraction,
                config.risk.max_position_pct * rust_decimal_macros::dec!(100)
            );
        }
    }

    Ok(())
}
