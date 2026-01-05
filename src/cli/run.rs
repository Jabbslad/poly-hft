//! Run command implementation - Lag Edges Trading Loop
//!
//! Implements the momentum-first lag detection strategy:
//! 1. Detect spot price momentum on Binance (>0.7% move)
//! 2. Check if Polymarket odds are lagging (still neutral)
//! 3. Enter positions when lag is detected
//! 4. Track positions until market resolution

use crate::config::Config;
use crate::data::{DataRecorder, RecorderConfig};
use crate::execution::{ExecutionEngine, Order, OrderType, PaperEngine};
use crate::feed::{BinanceFeed, PriceFeed};
use crate::market::{GammaClient, Market};
use crate::orderbook::{OrderBook, OrderBookManager, PolymarketClient};
use crate::risk::create_sizer;
use crate::signal::MomentumSignalDetector;
use chrono::Utc;
use clap::Args;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration as TokioDuration};

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Dry run mode (detect signals but don't execute)
    #[arg(long)]
    pub dry_run: bool,
}

impl RunArgs {
    pub async fn execute(&self, config: &Config) -> anyhow::Result<()> {
        tracing::info!("Starting lag edges paper trading...");

        // Initialize components
        let detector = Arc::new(RwLock::new(MomentumSignalDetector::from_configs(
            &config.momentum,
            &config.lag,
            dec!(0.005), // fee rate (0.5%)
            config.execution.slippage_estimate,
        )));

        let sizer = create_sizer(&config.sizing);
        let engine = PaperEngine::new(dec!(0.005));
        let _position_tracker = Arc::new(RwLock::new(crate::risk::PositionTracker::new()));
        let order_book_manager = Arc::new(RwLock::new(OrderBookManager::new()));
        let active_markets: Arc<RwLock<HashMap<String, Market>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let bankroll = config.risk.initial_bankroll;

        // Initialize data recorder if capture is enabled
        let recorder: Option<Arc<DataRecorder>> = if config.data.capture_enabled {
            let recorder_config = RecorderConfig {
                output_dir: config.data.output_dir.clone(),
                ..Default::default()
            };
            tracing::info!(
                output_dir = %config.data.output_dir.display(),
                "Data capture enabled"
            );
            Some(Arc::new(DataRecorder::new(recorder_config)))
        } else {
            None
        };

        // Start Binance price feed
        let feed = BinanceFeed::new(&config.feed.symbol);
        let mut price_rx = feed.subscribe().await?;

        // Polymarket client for order book subscriptions
        let polymarket_client = PolymarketClient::new();

        tracing::info!(
            symbol = %config.feed.symbol,
            bankroll = %bankroll,
            sizing_mode = %sizer.mode_name(),
            capture_enabled = config.data.capture_enabled,
            "Trading loop initialized"
        );

        // Spawn market discovery task
        let markets_clone = Arc::clone(&active_markets);
        let order_books_clone = Arc::clone(&order_book_manager);
        let recorder_clone = recorder.clone();
        let gamma_client = GammaClient::new();
        tokio::spawn(async move {
            let mut refresh_interval = interval(TokioDuration::from_secs(30));
            let mut subscribed_tokens: Vec<String> = Vec::new();

            loop {
                refresh_interval.tick().await;
                match gamma_client.fetch_btc_markets().await {
                    Ok(markets) => {
                        let mut active = markets_clone.write().await;
                        active.clear();

                        // Collect token IDs for subscription
                        let mut new_tokens: Vec<String> = Vec::new();
                        for market in &markets {
                            active.insert(market.condition_id.clone(), market.clone());
                            if !subscribed_tokens.contains(&market.yes_token_id) {
                                new_tokens.push(market.yes_token_id.clone());
                            }
                        }
                        tracing::debug!(count = active.len(), "Updated active markets");

                        // Subscribe to new tokens if any
                        if !new_tokens.is_empty() {
                            tracing::info!(
                                token_count = new_tokens.len(),
                                "Subscribing to new Polymarket order books"
                            );

                            match polymarket_client.subscribe(new_tokens.clone()).await {
                                Ok(mut orderbook_rx) => {
                                    subscribed_tokens.extend(new_tokens);

                                    // Spawn task to process order book updates
                                    let books_clone = Arc::clone(&order_books_clone);
                                    let rec_clone = recorder_clone.clone();
                                    tokio::spawn(async move {
                                        while let Some(book) = orderbook_rx.recv().await {
                                            // Record order book if capture enabled
                                            if let Some(ref rec) = rec_clone {
                                                if let Err(e) = rec.record_orderbook_async(book.clone()).await {
                                                    tracing::warn!(error = %e, "Failed to record order book");
                                                }
                                            }

                                            // Update order book manager
                                            let mut manager = books_clone.write().await;
                                            tracing::trace!(
                                                token_id = %book.token_id,
                                                best_bid = ?book.best_bid(),
                                                best_ask = ?book.best_ask(),
                                                "Order book update"
                                            );
                                            manager.update(book);
                                        }
                                    });
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "Failed to subscribe to order books");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to fetch markets");
                    }
                }
            }
        });

        // Main trading loop
        let mut signals_generated = 0u64;
        let mut trades_executed = 0u64;
        let mut ticks_captured = 0u64;

        while let Some(tick) = price_rx.recv().await {
            // Record tick if capture is enabled
            if let Some(ref rec) = recorder {
                if let Err(e) = rec.record_price_async(tick.clone()).await {
                    tracing::warn!(error = %e, "Failed to record price tick");
                } else {
                    ticks_captured += 1;
                }
            }

            // Update momentum detector with new price
            {
                let mut det = detector.write().await;
                det.update_price(tick.timestamp, tick.price);
            }

            // Log price periodically (every 100 ticks or so)
            if self.verbose && ticks_captured.is_multiple_of(100) {
                tracing::debug!(
                    price = %tick.price,
                    symbol = %tick.symbol,
                    ticks_captured = ticks_captured,
                    "Price tick"
                );
            }

            // Check for signals in each active market
            let markets = active_markets.read().await;
            for market in markets.values() {
                // Skip markets outside trading window
                let now = Utc::now();
                let seconds_since_open = (now - market.open_time).num_seconds();
                let seconds_until_close = (market.close_time - now).num_seconds();

                if seconds_since_open < config.lag.min_seconds_after_open as i64 {
                    continue; // Too early
                }
                if seconds_until_close < config.lag.max_seconds_before_close as i64 {
                    continue; // Too close to expiry
                }

                // Get order book for this market's YES token
                let books = order_book_manager.read().await;
                let orderbook = match books.get(&market.yes_token_id) {
                    Some(ob) => ob.clone(),
                    None => {
                        // Create a synthetic order book with neutral odds for testing
                        // In production, we'd skip if no real order book
                        OrderBook::new(market.yes_token_id.clone())
                    }
                };

                // Detect signal
                let signal = {
                    let mut det = detector.write().await;
                    det.detect(market, &orderbook)
                };

                if let Some(signal) = signal {
                    signals_generated += 1;

                    tracing::info!(
                        market = %market.condition_id,
                        side = ?signal.side,
                        edge = %signal.adjusted_edge,
                        fair_value = %signal.fair_value,
                        market_price = %signal.market_price,
                        "Signal detected!"
                    );

                    // Execute trade if not dry run
                    if !self.dry_run {
                        // Get lag signal for sizing (we need to reconstruct it)
                        let mut det = detector.write().await;
                        if let Some(momentum) =
                            det.momentum_detector_mut().detect(market.open_price)
                        {
                            let odds = crate::lag::OddsState::from_yes_price(signal.market_price);
                            let lag_signal = crate::lag::LagSignal::new(
                                match signal.side {
                                    crate::signal::Side::Yes => crate::lag::TradeSide::Yes,
                                    crate::signal::Side::No => crate::lag::TradeSide::No,
                                },
                                signal.adjusted_edge,
                                signal.fair_value,
                                signal.market_price,
                                momentum,
                                odds,
                                seconds_since_open,
                                seconds_until_close,
                            );

                            let size = sizer.calculate(&lag_signal, bankroll);

                            // Create and submit order
                            let token_id = match signal.side {
                                crate::signal::Side::Yes => &market.yes_token_id,
                                crate::signal::Side::No => &market.no_token_id,
                            };

                            let order = Order {
                                token_id: token_id.clone(),
                                side: signal.side,
                                price: signal.market_price,
                                size,
                                order_type: OrderType::Market,
                            };

                            match engine.submit_order(order).await {
                                Ok(order_id) => {
                                    trades_executed += 1;
                                    tracing::info!(
                                        ?order_id,
                                        size = %size,
                                        "Trade executed"
                                    );
                                }
                                Err(e) => {
                                    tracing::error!(error = %e, "Failed to execute trade");
                                }
                            }
                        }
                    }
                }
            }
        }

        tracing::info!(
            signals = signals_generated,
            trades = trades_executed,
            ticks_captured = ticks_captured,
            "Trading loop ended"
        );

        Ok(())
    }
}

/// Trading loop state for monitoring
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct TradingStats {
    pub ticks_processed: u64,
    pub signals_generated: u64,
    pub trades_executed: u64,
    pub current_positions: usize,
}
