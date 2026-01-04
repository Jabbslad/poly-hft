//! Run command implementation - Spread Capture Strategy

use crate::market::{GammaClient, Market, MarketTracker, MarketTrackerImpl};
use crate::orderbook::{OrderBook, PolymarketClient};
use crate::signal::{MarketBooks, SpreadConfig, SpreadDetector, SpreadSignal};
use anyhow::Result;
use chrono::Utc;
use clap::Args;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Minimum profit percentage to trade (default: 2%)
    #[arg(long, default_value = "0.02")]
    pub min_profit: Decimal,

    /// Position size per leg in USD (default: $5)
    #[arg(long, default_value = "5")]
    pub size: Decimal,

    /// Dry run - don't execute trades
    #[arg(long)]
    pub dry_run: bool,
}

/// State for tracking order books and positions
struct SpreadState {
    /// Order books by token_id
    order_books: HashMap<String, OrderBook>,
    /// Spread detector
    detector: SpreadDetector,
    /// Active positions by market condition_id
    positions: HashMap<String, SpreadPosition>,
    /// Total P&L
    total_pnl: Decimal,
    /// Total trades executed
    trade_count: u64,
}

/// A spread position
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used for tracking, will be used in live execution
struct SpreadPosition {
    pub market_id: String,
    pub yes_price: Decimal,
    pub no_price: Decimal,
    pub size_per_leg: Decimal,
    pub expected_profit: Decimal,
    pub opened_at: chrono::DateTime<Utc>,
}

impl RunArgs {
    pub async fn execute(&self) -> Result<()> {
        tracing::info!("Starting spread capture strategy...");
        tracing::info!(
            min_profit_pct = %self.min_profit,
            size_per_leg = %self.size,
            dry_run = self.dry_run,
            "Configuration"
        );

        // Initialize components
        let gamma_client = GammaClient::new();
        let market_tracker = Arc::new(MarketTrackerImpl::new(gamma_client));
        let _polymarket_client = PolymarketClient::new();

        // Configure spread detector
        let config = SpreadConfig {
            min_profit_pct: self.min_profit,
            fee_rate_per_side: dec!(0.005), // 0.5% per side
            max_book_age_ms: 2000,
            base_size_usd: self.size,
            max_positions: 50,
        };

        let state = Arc::new(RwLock::new(SpreadState {
            order_books: HashMap::new(),
            detector: SpreadDetector::with_config(config),
            positions: HashMap::new(),
            total_pnl: Decimal::ZERO,
            trade_count: 0,
        }));

        // Main loop
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        let mut check_interval = tokio::time::interval(std::time::Duration::from_millis(100));

        loop {
            tokio::select! {
                // Refresh markets periodically
                _ = interval.tick() => {
                    if let Err(e) = market_tracker.refresh().await {
                        tracing::warn!(error = %e, "Failed to refresh markets");
                        continue;
                    }

                    let markets = match market_tracker.get_active_markets().await {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to get markets");
                            continue;
                        }
                    };

                    tracing::info!(count = markets.len(), "Active markets");

                    // Log market info
                    for market in &markets {
                        tracing::debug!(
                            condition_id = %market.condition_id,
                            yes_token = %market.yes_token_id,
                            no_token = %market.no_token_id,
                            close_time = %market.close_time,
                            "Market"
                        );
                    }
                }

                // Check for spread opportunities
                _ = check_interval.tick() => {
                    let markets = match market_tracker.get_active_markets().await {
                        Ok(m) => m,
                        Err(_) => continue,
                    };

                    for market in markets {
                        if market.close_time <= Utc::now() {
                            // Market expired, check if we had a position
                            self.handle_market_expiry(&state, &market).await;
                            continue;
                        }

                        // Check for spread opportunity
                        if let Some(signal) = self.check_spread(&state, &market).await {
                            self.handle_signal(&state, signal, self.dry_run).await;
                        }
                    }
                }
            }
        }
    }

    /// Check for spread opportunity on a market
    async fn check_spread(
        &self,
        state: &Arc<RwLock<SpreadState>>,
        market: &Market,
    ) -> Option<SpreadSignal> {
        let state = state.read().await;

        // Get both order books
        let yes_book = state.order_books.get(&market.yes_token_id)?;
        let no_book = state.order_books.get(&market.no_token_id)?;

        let books = MarketBooks::new(yes_book.clone(), no_book.clone());

        // Check if we can take more positions
        if !state.detector.can_take_position(&market.condition_id) {
            return None;
        }

        // Detect spread opportunity
        state.detector.detect(market, &books)
    }

    /// Handle a spread signal
    async fn handle_signal(
        &self,
        state: &Arc<RwLock<SpreadState>>,
        signal: SpreadSignal,
        dry_run: bool,
    ) {
        tracing::info!(
            market = %signal.market.condition_id,
            yes_price = %signal.yes_price,
            no_price = %signal.no_price,
            total_cost = %signal.total_cost,
            net_profit = %signal.net_profit,
            profit_pct = %signal.profit_pct,
            size = %signal.size_per_leg_usd,
            "Spread opportunity!"
        );

        if dry_run {
            tracing::info!("DRY RUN: Would execute spread trade");
            return;
        }

        // Execute the trade (paper trading for now)
        let position = SpreadPosition {
            market_id: signal.market.condition_id.clone(),
            yes_price: signal.yes_price,
            no_price: signal.no_price,
            size_per_leg: signal.size_per_leg_usd,
            expected_profit: signal.net_profit * signal.size_per_leg_usd,
            opened_at: Utc::now(),
        };

        let mut state = state.write().await;
        state.detector.add_position(&signal.market.condition_id);
        state
            .positions
            .insert(signal.market.condition_id.clone(), position.clone());
        state.trade_count += 1;

        tracing::info!(
            market = %signal.market.condition_id,
            expected_profit = %position.expected_profit,
            trade_count = state.trade_count,
            "Position opened"
        );
    }

    /// Handle market expiry - close position and record P&L
    async fn handle_market_expiry(&self, state: &Arc<RwLock<SpreadState>>, market: &Market) {
        let mut state = state.write().await;

        if let Some(position) = state.positions.remove(&market.condition_id) {
            // In spread capture, we always profit if we got both sides
            let profit = position.expected_profit;
            state.total_pnl += profit;
            state.detector.remove_position(&market.condition_id);

            tracing::info!(
                market = %market.condition_id,
                profit = %profit,
                total_pnl = %state.total_pnl,
                "Position closed (market expired)"
            );
        }
    }
}
