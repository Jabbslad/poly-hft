# PRD: Polymarket HFT Bot (poly-hft)

## Overview

A high-frequency trading bot that exploits pricing lags in Polymarket's 15-minute BTC up/down binary markets. When spot prices move, there's a brief window where Polymarket odds remain stale. The bot calculates fair value from real-time Binance prices and trades the discrepancy.

## Strategy Summary

1. Monitor real-time BTC spot price via Binance WebSocket
2. Track active 15-minute up/down markets on Polymarket
3. Calculate fair probability using Black-Scholes-style model (GBM)
4. Compare fair value to current market prices
5. When edge exceeds threshold (accounting for fees/slippage), execute trade
6. Close positions as market converges or at settlement

## MVP Scope (Phase 1)

**Paper Trading Mode**: Full logic with simulated execution against live data.

### In Scope
- BTC 15-minute up/down markets only
- Binance WebSocket price feed
- Paper trading execution (no real orders)
- Tick data capture to Parquet
- CLI interface with subcommands
- Full observability stack
- Backtesting capability

### Out of Scope (Phase 2+)
- Real execution against Polymarket CLOB
- ETH and other assets
- Multiple exchange price feeds
- Automated position management
- Wallet/key management UI

---

## Technical Requirements

### Performance Targets
- Price feed latency: < 50ms from Binance
- Signal generation: < 10ms after price update
- End-to-end loop: < 100ms (paper), < 200ms (live)

### Quality Standards
- Test coverage: >= 75%
- Idiomatic Rust (clippy clean, rustfmt)
- Documentation for public APIs

### Risk Parameters (Configurable)
- Max position size: 1% of bankroll per trade
- Kelly fraction: 0.25 (quarter Kelly)
- Minimum edge threshold: 0.5% (after estimated fees)
- Max concurrent positions: 3

---

## Architecture

```
+------------------+     +------------------+     +------------------+
|  Binance WS      |---->|  Price Engine    |---->|  Fair Value      |
|  (BTC/USDT)      |     |  (tick capture)  |     |  Calculator      |
+------------------+     +------------------+     +------------------+
                                                          |
+------------------+     +------------------+              v
|  Polymarket WS   |---->|  Order Book      |---->+------------------+
|  (market channel)|     |  Aggregator      |     |  Signal          |
+------------------+     +------------------+     |  Generator       |
                                                  +------------------+
+------------------+                                       |
|  Gamma API       |---->  Market Discovery                v
|  (15-min markets)|                              +------------------+
+------------------+                              |  Execution       |
                                                  |  Engine (Paper)  |
+------------------+                              +------------------+
|  CLI Interface   |                                       |
|  (clap)          |                                       v
+------------------+                              +------------------+
                                                  |  Risk Manager    |
+------------------+                              |  Position Tracker|
|  Observability   |<-----------------------------+------------------+
|  (metrics/traces)|
+------------------+
```

For detailed data flow diagrams, component interactions, and module dependencies, see [Architecture Documentation](./architecture.md).

---

## Module Breakdown

### 1. Price Feed (`src/feed/`)
**Purpose**: Real-time BTC price from Binance WebSocket

**Components**:
- `binance.rs` - WebSocket client for `btcusdt@trade` stream
- `types.rs` - Price tick types with timestamps

**Key Dependencies**: `tokio-tungstenite`, `serde`

**Binance WebSocket**:
- Endpoint: `wss://stream.binance.com:9443/ws/btcusdt@trade`
- Authentication: None required (public market data stream)

**Interface**:
```rust
pub trait PriceFeed: Send + Sync {
    async fn subscribe(&self) -> Result<mpsc::Receiver<PriceTick>>;
}

pub struct PriceTick {
    pub symbol: String,
    pub price: Decimal,
    pub timestamp: DateTime<Utc>,
    pub exchange_ts: DateTime<Utc>,  // Binance timestamp
}
```

### 2. Market Discovery (`src/market/`)
**Purpose**: Find and track active 15-minute BTC up/down markets

**Components**:
- `gamma.rs` - Gamma API client for market discovery
- `tracker.rs` - Tracks active markets, handles rollovers

**Interface**:
```rust
pub struct Market {
    pub condition_id: String,
    pub yes_token_id: String,
    pub no_token_id: String,
    pub open_price: Decimal,       // BTC price at market open
    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
}

pub trait MarketTracker: Send + Sync {
    async fn get_active_markets(&self) -> Result<Vec<Market>>;
    async fn refresh(&self) -> Result<()>;
}
```

### 3. Order Book (`src/orderbook/`)
**Purpose**: Real-time order book from Polymarket WebSocket

**Components**:
- `client.rs` - WebSocket client for market channel
- `book.rs` - Order book state management (L2 aggregated)

**Interface**:
```rust
pub struct OrderBook {
    pub token_id: String,
    pub bids: Vec<PriceLevel>,  // Sorted best->worst
    pub asks: Vec<PriceLevel>,
    pub updated_at: DateTime<Utc>,
}

pub struct PriceLevel {
    pub price: Decimal,
    pub size: Decimal,
}
```

### 4. Fair Value Model (`src/model/`)
**Purpose**: Calculate theoretical fair value for Yes/No tokens

**Components**:
- `gbm.rs` - Geometric Brownian Motion probability model
- `volatility.rs` - Rolling realized volatility estimator

**Model**:
```
P(up) = N(d2)
where:
  d2 = (ln(S/K) - 0.5*sigma^2*T) / (sigma*sqrt(T))
  S = current spot price
  K = market open price
  T = time to expiry (in years)
  sigma = annualized volatility
```

**Interface**:
```rust
pub trait FairValueModel: Send + Sync {
    fn calculate(&self, params: FairValueParams) -> FairValue;
}

pub struct FairValueParams {
    pub current_price: Decimal,
    pub open_price: Decimal,
    pub time_to_expiry: Duration,
    pub volatility: Decimal,
}

pub struct FairValue {
    pub yes_prob: Decimal,
    pub no_prob: Decimal,
    pub confidence: Decimal,  // Based on vol certainty
}
```

### 5. Signal Generator (`src/signal/`)
**Purpose**: Detect tradeable pricing discrepancies

**Components**:
- `detector.rs` - Compares fair value to market prices
- `filter.rs` - Applies edge thresholds and filters
- `types.rs` - Signal types and enums

**Signal Detection Logic**:

1. **Edge Calculation**:
   ```
   raw_edge = fair_value - market_price
   adjusted_edge = raw_edge - estimated_fees - slippage_estimate
   ```

2. **Signal Filters** (all must pass):
   - `min_edge_threshold`: Reject if adjusted_edge < 0.5% (configurable)
   - `max_edge_threshold`: Reject if adjusted_edge > 10% (likely stale/bad data)
   - `min_time_to_expiry`: Reject if < 60s to settlement (vol spikes, uncertain fills)
   - `max_time_to_expiry`: Reject if > 14m from open (edge typically appears post-reset)
   - `min_liquidity`: Reject if order book depth < min_order_size at target price
   - `volatility_sanity`: Reject if estimated vol < 10% or > 200% annualized

3. **Confidence Scoring**:
   ```rust
   pub fn calculate_confidence(params: &SignalParams) -> Decimal {
       let vol_confidence = 1.0 - (vol_std_error / vol_estimate).min(1.0);
       let time_confidence = (time_to_expiry / 15_minutes).min(1.0);
       let liquidity_confidence = (available_liquidity / target_size).min(1.0);

       (vol_confidence * 0.4 + time_confidence * 0.3 + liquidity_confidence * 0.3)
   }
   ```

4. **Signal Priority** (when multiple signals exist):
   - Sort by `adjusted_edge * confidence` descending
   - Respect `max_concurrent_positions` limit

**Post-Reset Detection**:
The strategy specifically targets the first 1-2 minutes after a new 15-min market opens, when:
- Liquidity providers are slower to update quotes
- Order book may be thin/stale
- Spot price has already moved from the settlement price

```rust
pub struct ResetDetector {
    pub last_market_close: HashMap<String, DateTime<Utc>>,
}

impl ResetDetector {
    // Returns true if market opened within `window` duration
    pub fn is_post_reset(&self, market: &Market, window: Duration) -> bool;
}
```

**Interface**:
```rust
#[derive(Debug, Clone)]
pub struct Signal {
    pub id: Uuid,
    pub market: Market,
    pub side: Side,           // Buy Yes or Buy No
    pub fair_value: Decimal,
    pub market_price: Decimal,
    pub raw_edge: Decimal,
    pub adjusted_edge: Decimal,  // After fees/slippage
    pub confidence: Decimal,
    pub reason: SignalReason,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum SignalReason {
    PostResetLag,        // Market just opened, prices lagging
    SpotDivergence,      // Spot moved significantly, odds stale
    VolatilitySpike,     // Vol increased, fair value shifted
}

#[derive(Debug, Clone)]
pub enum FilterResult {
    Pass,
    Reject(RejectReason),
}

#[derive(Debug, Clone)]
pub enum RejectReason {
    EdgeTooSmall(Decimal),
    EdgeTooLarge(Decimal),
    InsufficientLiquidity(Decimal),
    TooCloseToExpiry(Duration),
    VolatilityOutOfRange(Decimal),
    MaxPositionsReached,
}
```

### 6. Execution Engine (`src/execution/`)
**Purpose**: Execute trades (paper mode for MVP)

**Components**:
- `paper.rs` - Simulated execution with realistic fills
- `types.rs` - Order and fill types

**Interface**:
```rust
pub trait ExecutionEngine: Send + Sync {
    async fn submit_order(&self, order: Order) -> Result<OrderId>;
    async fn cancel_order(&self, id: OrderId) -> Result<()>;
    async fn get_fills(&self) -> Result<Vec<Fill>>;
}

pub struct Order {
    pub token_id: String,
    pub side: Side,
    pub price: Decimal,
    pub size: Decimal,
    pub order_type: OrderType,
}
```

### 7. Risk Manager (`src/risk/`)
**Purpose**: Position sizing and risk controls

**Components**:
- `kelly.rs` - Kelly criterion position sizing
- `limits.rs` - Position limits, drawdown controls
- `position.rs` - Position tracking and P&L
- `types.rs` - Risk-related types

**Kelly Criterion Position Sizing**:

The Kelly formula for binary outcomes:
```
f* = (p * b - q) / b

where:
  f* = fraction of bankroll to bet
  p  = probability of winning (our fair value)
  q  = 1 - p (probability of losing)
  b  = odds received (payout ratio)
```

For Polymarket binary markets where shares pay $1 if correct:
```
b = (1 - market_price) / market_price  # for Yes side
f* = (fair_value - market_price) / (1 - market_price)
```

**Fractional Kelly**:
Full Kelly is too aggressive for most strategies. We use quarter Kelly (0.25x) by default:
```rust
pub struct KellyCalculator {
    pub fraction: Decimal,  // 0.25 = quarter Kelly
    pub max_bet_pct: Decimal,  // Hard cap at 1% of bankroll
}

impl KellyCalculator {
    pub fn calculate(&self, signal: &Signal, bankroll: Decimal) -> Decimal {
        let edge = signal.fair_value - signal.market_price;
        let odds = (Decimal::ONE - signal.market_price) / signal.market_price;

        let kelly_fraction = edge / (Decimal::ONE - signal.market_price);
        let adjusted = kelly_fraction * self.fraction;

        // Apply hard cap
        let max_size = bankroll * self.max_bet_pct;
        (adjusted * bankroll).min(max_size)
    }
}
```

**Position Limits**:
```rust
pub struct PositionLimits {
    pub max_position_pct: Decimal,       // Max 1% of bankroll per position
    pub max_concurrent_positions: usize, // Max 3 simultaneous positions
    pub max_daily_loss_pct: Decimal,     // Stop trading if down 5% today
    pub max_drawdown_pct: Decimal,       // Stop trading if down 10% from peak
    pub max_exposure_pct: Decimal,       // Max 10% total capital at risk
}
```

**Drawdown Controls**:
```rust
pub struct DrawdownMonitor {
    pub peak_equity: Decimal,
    pub current_equity: Decimal,
    pub daily_start_equity: Decimal,
    pub daily_pnl: Decimal,
}

impl DrawdownMonitor {
    pub fn update(&mut self, new_equity: Decimal);
    pub fn current_drawdown(&self) -> Decimal;
    pub fn daily_drawdown(&self) -> Decimal;
    pub fn should_halt(&self, limits: &PositionLimits) -> Option<HaltReason>;
}

pub enum HaltReason {
    MaxDailyLossReached(Decimal),
    MaxDrawdownReached(Decimal),
    MaxExposureReached(Decimal),
}
```

**Position Tracking**:
```rust
pub struct Position {
    pub id: Uuid,
    pub market: Market,
    pub side: Side,
    pub entry_price: Decimal,
    pub size: Decimal,
    pub entry_time: DateTime<Utc>,
    pub unrealized_pnl: Decimal,
}

pub struct PositionTracker {
    pub open_positions: HashMap<Uuid, Position>,
    pub closed_positions: Vec<ClosedPosition>,
    pub total_exposure: Decimal,
}

impl PositionTracker {
    pub fn open(&mut self, signal: &Signal, fill: &Fill) -> Position;
    pub fn close(&mut self, position_id: Uuid, fill: &Fill) -> ClosedPosition;
    pub fn update_mark(&mut self, market_id: &str, current_price: Decimal);
    pub fn total_pnl(&self) -> Decimal;
}
```

**Interface**:
```rust
pub trait RiskManager: Send + Sync {
    fn calculate_size(&self, signal: &Signal, bankroll: Decimal) -> Decimal;
    fn check_limits(&self, order: &Order, tracker: &PositionTracker) -> Result<(), RiskError>;
    fn should_halt(&self) -> Option<HaltReason>;
}

pub enum RiskError {
    PositionTooLarge(Decimal),
    MaxPositionsReached,
    MaxExposureReached,
    TradingHalted(HaltReason),
}
```

### 8. Data Capture (`src/data/`)
**Purpose**: Store tick data for backtesting

**Components**:
- `recorder.rs` - Async writer for price/orderbook ticks
- `parquet.rs` - Parquet file writer with rotation

**Schema** (Parquet):
```
price_ticks.parquet:
  - timestamp: TIMESTAMP_MICROS
  - symbol: STRING
  - price: DECIMAL(18,8)
  - exchange_ts: TIMESTAMP_MICROS

orderbook_ticks.parquet:
  - timestamp: TIMESTAMP_MICROS
  - token_id: STRING
  - bid_price_0..4: DECIMAL(18,8)
  - bid_size_0..4: DECIMAL(18,8)
  - ask_price_0..4: DECIMAL(18,8)
  - ask_size_0..4: DECIMAL(18,8)

signals.parquet:
  - timestamp: TIMESTAMP_MICROS
  - market_id: STRING
  - side: STRING
  - fair_value: DECIMAL(18,8)
  - market_price: DECIMAL(18,8)
  - edge: DECIMAL(18,8)
  - action: STRING (signal/trade/skip)
```

### 9. Backtester (`src/backtest/`)
**Purpose**: Replay historical data and simulate strategy with realistic execution

**Components**:
- `replay.rs` - Event-driven Parquet reader with time synchronization
- `simulator.rs` - Strategy simulation engine
- `execution_model.rs` - Queue position and fill simulation
- `analytics.rs` - Performance metrics and reporting

**Data Sources**:
- Captured tick data only (Parquet files from data module)
- Merges price_ticks + orderbook_ticks streams by timestamp

**Event-Driven Replay**:
```rust
pub enum BacktestEvent {
    PriceTick(PriceTick),
    OrderBookUpdate(OrderBookSnapshot),
    MarketOpen(Market),
    MarketClose(Market),
}

pub struct EventStream {
    // Merges multiple Parquet files, yields events in timestamp order
    pub fn next(&mut self) -> Option<(DateTime<Utc>, BacktestEvent)>;
}
```

**Queue Position Simulation**:
- Track queue position for limit orders based on order book state
- Model partial fills when price trades through
- Simulate latency between signal and order submission
- Account for order book depth consumption

```rust
pub struct QueueSimulator {
    pub latency_ms: u64,           // Simulated order latency
    pub queue_position: HashMap<OrderId, QueueState>,
}

pub struct QueueState {
    pub price_level: Decimal,
    pub ahead_size: Decimal,       // Size ahead in queue
    pub our_size: Decimal,
    pub filled: Decimal,
}

impl QueueSimulator {
    // Called on each order book update to advance queue positions
    pub fn process_book_update(&mut self, book: &OrderBook) -> Vec<Fill>;
}
```

**Output/Reporting**:

1. **Summary Statistics**:
   - Total P&L, Net P&L (after fees)
   - Sharpe ratio, Sortino ratio
   - Win rate, profit factor
   - Max drawdown ($ and %)
   - Total trades, avg trade duration
   - Avg edge captured vs theoretical

2. **Trade Log** (Parquet):
   ```
   backtest_trades.parquet:
     - trade_id: STRING
     - signal_time: TIMESTAMP_MICROS
     - fill_time: TIMESTAMP_MICROS
     - market_id: STRING
     - side: STRING
     - signal_edge: DECIMAL
     - entry_price: DECIMAL
     - exit_price: DECIMAL
     - size: DECIMAL
     - pnl: DECIMAL
     - fees: DECIMAL
   ```

3. **Equity Curve Data** (for visualization):
   ```
   equity_curve.parquet:
     - timestamp: TIMESTAMP_MICROS
     - equity: DECIMAL
     - drawdown: DECIMAL
     - open_positions: INT32
   ```

**Interface**:
```rust
pub struct BacktestConfig {
    pub data_dir: PathBuf,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub initial_capital: Decimal,
    pub latency_ms: u64,
    pub fee_rate: Decimal,
}

pub struct BacktestResult {
    pub summary: BacktestSummary,
    pub trades_path: PathBuf,      // Path to trades parquet
    pub equity_path: PathBuf,      // Path to equity curve parquet
}

pub trait Backtester {
    async fn run(&self, config: BacktestConfig) -> Result<BacktestResult>;
}
```

### 10. CLI (`src/cli/`)
**Purpose**: User interface

**Commands**:
```
poly-hft run              # Start paper trading
poly-hft status           # Show current state
poly-hft capture          # Data capture only (no trading)
poly-hft config           # Show/edit configuration

poly-hft backtest         # Run backtest with default settings
  --data-dir <PATH>       # Directory containing Parquet files
  --start <DATETIME>      # Start time filter (ISO 8601)
  --end <DATETIME>        # End time filter (ISO 8601)
  --capital <AMOUNT>      # Initial capital (default: from config)
  --latency <MS>          # Simulated latency in ms (default: 50)
  --output <PATH>         # Output directory for results
  --format <FMT>          # Output format: json|table (default: table)
```

**Backtest Output Example**:
```
══════════════════════════════════════════════════════
               BACKTEST RESULTS
══════════════════════════════════════════════════════
Period:           2025-01-01 00:00 - 2025-01-07 23:59
Initial Capital:  $500.00
Final Equity:     $623.45

PERFORMANCE
───────────────────────────────────────────────────────
Net P&L:          +$123.45 (+24.69%)
Sharpe Ratio:     2.34
Sortino Ratio:    3.12
Max Drawdown:     -$45.23 (-7.25%)
Win Rate:         58.3%
Profit Factor:    1.89

ACTIVITY
───────────────────────────────────────────────────────
Total Trades:     342
Avg Duration:     4m 23s
Avg Edge:         0.82%
Fees Paid:        $12.34

Output files:
  - trades:  ./output/backtest_trades.parquet
  - equity:  ./output/equity_curve.parquet
══════════════════════════════════════════════════════
```

**Dependencies**: `clap` with derive

### 11. Observability (`src/telemetry/`)
**Purpose**: Metrics, logs, traces for full system visibility

**Components**:
- `metrics.rs` - Prometheus metrics (latency histograms, counters, gauges)
- `tracing.rs` - OpenTelemetry distributed tracing setup
- `logging.rs` - Structured JSON logging with context

**Metrics (Prometheus)**:

```rust
// Latency histograms (milliseconds)
polyhft_price_feed_latency_ms        // Binance event time -> bot receive time
polyhft_orderbook_update_latency_ms  // Polymarket -> bot
polyhft_signal_generation_latency_ms // Price tick -> signal emit
polyhft_order_submission_latency_ms  // Signal -> order sent (paper: simulated)

// Counters
polyhft_price_ticks_total            // Total price updates received
polyhft_orderbook_updates_total      // Total order book updates
polyhft_signals_total{side,reason,action}  // Signals by side/reason/pass|reject
polyhft_orders_total{side,status}    // Orders by outcome
polyhft_fills_total{side}            // Executed fills
polyhft_ws_reconnects_total{feed}    // WebSocket reconnection count
polyhft_errors_total{component,type} // Errors by component

// Gauges
polyhft_equity_usd                   // Current equity value
polyhft_unrealized_pnl_usd           // Open position P&L
polyhft_realized_pnl_usd             // Closed position P&L
polyhft_open_positions               // Number of open positions
polyhft_total_exposure_usd           // Total capital at risk
polyhft_drawdown_pct                 // Current drawdown from peak
polyhft_daily_pnl_usd                // Today's P&L
polyhft_current_volatility           // Estimated BTC volatility
polyhft_active_markets               // Number of tracked markets
```

**Metric Labels**:
```
side:    yes | no
reason:  post_reset_lag | spot_divergence | volatility_spike
action:  pass | reject_edge_small | reject_edge_large | reject_liquidity | ...
status:  pending | filled | cancelled | rejected
feed:    binance | polymarket
component: feed | market | orderbook | signal | execution | risk
```

**Tracing (OpenTelemetry)**:

Distributed traces for request flow analysis:

```rust
// Span hierarchy:
trading_loop                         // Root span for each tick
  ├── process_price_tick             // Handle Binance update
  │   ├── update_volatility          // Recalculate vol estimate
  │   └── update_fair_value          // Recalculate all market fair values
  ├── process_orderbook_update       // Handle Polymarket update
  ├── generate_signals               // Check for trading opportunities
  │   ├── calculate_edge             // Per-market edge calculation
  │   └── apply_filters              // Run filter chain
  ├── evaluate_risk                  // Check position limits
  └── submit_order                   // Execute trade (if signal passes)
      ├── calculate_size             // Kelly sizing
      └── place_order                // API call (paper: simulated)
```

**Span Attributes**:
```rust
#[instrument(
    skip(self),
    fields(
        market_id = %market.condition_id,
        spot_price = %current_price,
        fair_value = %fair_value,
        market_price = %market_price,
        edge = %edge,
    )
)]
async fn generate_signal(&self, market: &Market, ...) -> Option<Signal>
```

**Structured Logging**:

JSON format with consistent fields:
```json
{
  "timestamp": "2025-01-04T12:34:56.789Z",
  "level": "INFO",
  "target": "polyhft::signal::detector",
  "message": "Signal generated",
  "span": {"trading_loop": "abc123", "generate_signals": "def456"},
  "fields": {
    "market_id": "0x1234...",
    "side": "yes",
    "edge": 0.0082,
    "confidence": 0.73,
    "reason": "post_reset_lag"
  }
}
```

**Log Levels**:
- `ERROR`: System failures, halted trading, data corruption
- `WARN`: Recoverable issues, reconnections, rejected signals
- `INFO`: Trades, signals, significant state changes
- `DEBUG`: Detailed flow, intermediate calculations
- `TRACE`: Every tick, every update (very verbose)

**Alerting Integration**:

The metrics endpoint enables alerting via Prometheus Alertmanager:

```yaml
# Example alert rules (not part of codebase, for user reference)
groups:
  - name: polyhft
    rules:
      - alert: HighDrawdown
        expr: polyhft_drawdown_pct > 0.05
        for: 1m
        labels:
          severity: warning

      - alert: TradingHalted
        expr: changes(polyhft_orders_total[5m]) == 0 AND polyhft_active_markets > 0
        for: 5m
        labels:
          severity: critical

      - alert: FeedDisconnected
        expr: rate(polyhft_price_ticks_total[1m]) == 0
        for: 30s
        labels:
          severity: critical
```

**Dashboard Panels** (Grafana JSON export available):

1. **Overview**: Equity curve, P&L, drawdown
2. **Latency**: Feed latency histograms, p50/p95/p99
3. **Trading Activity**: Signals/trades per minute, win rate rolling
4. **Risk**: Exposure, position count, daily P&L
5. **System Health**: Reconnects, errors, active feeds

**Interface**:
```rust
pub struct TelemetryConfig {
    pub metrics_port: u16,           // Prometheus scrape port
    pub log_level: LevelFilter,
    pub log_format: LogFormat,       // Json | Pretty
    pub otlp_endpoint: Option<String>, // OpenTelemetry collector
    pub service_name: String,        // "polyhft"
}

pub fn init_telemetry(config: &TelemetryConfig) -> Result<TelemetryGuard>;

// Metric recording helpers
pub fn record_latency(metric: LatencyMetric, duration: Duration);
pub fn increment_counter(metric: CounterMetric, labels: &[(&str, &str)]);
pub fn set_gauge(metric: GaugeMetric, value: f64);
```

---

## Configuration

```toml
# config.toml

[feed]
exchange = "binance"
symbol = "BTCUSDT"

[market]
asset = "BTC"
interval = "15m"
refresh_interval_secs = 30

[model]
volatility_window_minutes = 30
min_time_to_expiry_secs = 60  # Don't trade last minute

[signal]
min_edge_threshold = 0.005    # 0.5%
max_edge_threshold = 0.10     # 10% (likely stale data)

[risk]
kelly_fraction = 0.25
max_position_pct = 0.01       # 1% of bankroll
max_concurrent_positions = 3
initial_bankroll = 500.0

[execution]
mode = "paper"                # paper | live
slippage_estimate = 0.001     # 0.1%

[data]
capture_enabled = true
output_dir = "./data"
rotation_interval = "1h"

[telemetry]
metrics_port = 9090
log_level = "info"
otlp_endpoint = "http://localhost:4317"
```

---

## Dependencies (Cargo.toml)

```toml
[package]
name = "poly-hft"
version = "0.1.0"
edition = "2021"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# Polymarket client
polymarket-client-sdk = { version = "0.3", features = ["ws", "gamma"] }

# Crypto signing
alloy = { version = "0.9", features = ["signer-local"] }

# HTTP client
reqwest = { version = "0.12", features = ["json"] }

# WebSocket
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }
futures-util = "0.3"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# Decimal math
rust_decimal = { version = "1.36", features = ["serde"] }
rust_decimal_macros = "1.36"

# Statistics
statrs = "0.17"

# Time
chrono = { version = "0.4", features = ["serde"] }

# CLI
clap = { version = "4", features = ["derive"] }

# Data storage
parquet = { version = "53", features = ["async"] }
arrow = "53"

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
tracing-opentelemetry = "0.28"
opentelemetry = "0.27"
opentelemetry-otlp = "0.27"
metrics = "0.24"
metrics-exporter-prometheus = "0.16"

# Error handling
thiserror = "2"
anyhow = "1"

[dev-dependencies]
# Pre-commit hooks - auto-installs on cargo build/test
# Requires .cargo-husky/hooks/pre-commit script (checked in)
cargo-husky = { version = "1", default-features = false, features = ["user-hooks"] }

tokio-test = "0.4"
mockall = "0.13"
criterion = { version = "0.5", features = ["async_tokio"] }
proptest = "1"
wiremock = "0.6"

[[bench]]
name = "fair_value"
harness = false
```

---

## Implementation Phases

### Phase 1: Foundation (Current MVP)
1. Project structure and configuration
2. Binance WebSocket price feed
3. Fair value model (GBM)
4. Parquet data capture
5. CLI skeleton
6. Basic observability

### Phase 2: Market Integration
1. Gamma API market discovery
2. Polymarket WebSocket order book
3. Signal generator
4. Paper execution engine

### Phase 3: Strategy Complete
1. Risk manager
2. Position tracker
3. Full backtester
4. End-to-end paper trading loop

### Phase 4: Production Ready
1. Live execution engine
2. Wallet integration
3. Advanced risk controls
4. Performance optimization

---

## Testing Strategy

### Unit Tests (target: 80%)
- Fair value model: Property-based tests for edge cases
- Risk manager: Kelly calculation correctness
- Signal generator: Edge detection accuracy

### Integration Tests
- WebSocket reconnection handling
- Market discovery polling
- Data capture file rotation

### End-to-End Tests
- Full paper trading loop with mock feeds
- Backtest reproducibility

### Benchmarks
- Fair value calculation latency
- Order book update processing
- Signal generation throughput

---

## File Structure

```
poly-hft/
├── Cargo.toml
├── CLAUDE.md
├── config.toml.example
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── config.rs
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── run.rs
│   │   ├── backtest.rs
│   │   └── capture.rs
│   ├── feed/
│   │   ├── mod.rs
│   │   ├── binance.rs
│   │   └── types.rs
│   ├── market/
│   │   ├── mod.rs
│   │   ├── gamma.rs
│   │   └── tracker.rs
│   ├── orderbook/
│   │   ├── mod.rs
│   │   ├── client.rs
│   │   └── book.rs
│   ├── model/
│   │   ├── mod.rs
│   │   ├── gbm.rs
│   │   └── volatility.rs
│   ├── signal/
│   │   ├── mod.rs
│   │   ├── detector.rs
│   │   ├── filter.rs
│   │   └── types.rs
│   ├── execution/
│   │   ├── mod.rs
│   │   ├── paper.rs
│   │   └── types.rs
│   ├── risk/
│   │   ├── mod.rs
│   │   ├── kelly.rs
│   │   ├── limits.rs
│   │   ├── position.rs
│   │   └── types.rs
│   ├── data/
│   │   ├── mod.rs
│   │   ├── recorder.rs
│   │   └── parquet.rs
│   ├── backtest/
│   │   ├── mod.rs
│   │   ├── replay.rs
│   │   ├── simulator.rs
│   │   ├── execution_model.rs
│   │   └── analytics.rs
│   └── telemetry/
│       ├── mod.rs
│       ├── metrics.rs
│       ├── tracing.rs
│       └── logging.rs
├── tests/
│   ├── integration/
│   │   ├── feed_test.rs
│   │   ├── market_test.rs
│   │   └── e2e_test.rs
│   └── fixtures/
│       └── sample_data/
├── benches/
│   └── fair_value.rs
└── data/           # Git-ignored, runtime data
```

---

## Design Decisions

1. **Volatility Model**: Using realized volatility only (rolling std dev of log returns from Binance). Simpler and more predictable than implied vol derivation.

2. **Execution Delay Handling**: Use maker orders where possible to avoid taker delay. For taker orders, add ~500ms buffer to edge calculations. See research findings below.

## Research Findings

### Polymarket Execution Delay Behavior (Researched: 2026-01-04)

**Architecture**: Polymarket uses a hybrid-decentralized CLOB with off-chain matching and on-chain settlement. Limit orders are entirely off-chain until matched.

**Latency Characteristics**:
| Component | Latency | Notes |
|-----------|---------|-------|
| WebSocket updates | < 50ms | Real-time market data |
| Order signing (Python) | ~1000ms | Unoptimized reference implementation |
| Order signing (Rust/Go) | < 100ms | Optimized implementations required for HFT |
| Taker delay | ~500ms | Market makers use 500ms cancel/replace intervals to account for this |
| Cloudflare overhead | Variable | Adds latency; no "magic 5ms servers" unless special access |
| Sports markets | 3000ms | Intentional delay to prevent live-score sniping |

**Key Insights**:
1. **500ms taker delay is real**: Market maker bots configure `CANCEL_REPLACE_INTERVAL_MS=500` to match this delay
2. **Limit orders are off-chain**: Using a Polygon RPC for limit orders adds unnecessary latency
3. **No atomic arbitrage**: Execution delays mean cross-venue arbitrage carries unhedged position risk
4. **Queue position matters**: Earlier orders at a price level get filled first

**Implications for poly-hft**:
- Prefer maker orders (post-only) to avoid taker delay
- Add 500ms buffer to edge calculations for taker orders
- Use Rust SDK (not Python) for order signing
- Target ~100ms end-to-end latency (achievable with optimized code)

**Rate Limits** (CLOB Trading):
| Endpoint | Burst (10s) | Sustained (10min) |
|----------|-------------|-------------------|
| POST /order | 3,500 (500/s) | 36,000 (60/s) |
| DELETE /order | 3,000 (300/s) | 30,000 (50/s) |
| POST /orders (batch) | 1,000 (100/s) | 15,000 (25/s) |

Note: Cloudflare throttles rather than rejects—requests are delayed/queued.

## Research Items (Pre-Implementation)

- [ ] Verify how Polymarket determines market open price (Chainlink oracle timestamp?)
- [x] Current Polymarket execution delay behavior (500ms rumor) — **Confirmed, see above**
- [ ] Current fee structure for takers vs makers
- [ ] Optimal edge threshold accounting for fees + slippage

---

## Success Metrics

- Paper trading Sharpe ratio > 2.0
- Win rate > 55%
- Average trade duration < 10 minutes
- System uptime > 99% during market hours
- Latency p99 < 100ms
