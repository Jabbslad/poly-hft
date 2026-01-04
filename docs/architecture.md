# poly-hft Architecture

## System Overview Diagram

```
                                    EXTERNAL DATA SOURCES
┌─────────────────────────────────────────────────────────────────────────────────┐
│                                                                                 │
│   ┌─────────────────┐      ┌─────────────────┐      ┌─────────────────┐        │
│   │   Binance WS    │      │  Polymarket WS  │      │   Gamma API     │        │
│   │  (BTC/USDT      │      │  (Order Book    │      │  (Market        │        │
│   │   Trade Stream) │      │   Updates)      │      │   Discovery)    │        │
│   └────────┬────────┘      └────────┬────────┘      └────────┬────────┘        │
│            │                        │                        │                  │
└────────────┼────────────────────────┼────────────────────────┼──────────────────┘
             │                        │                        │
             ▼                        ▼                        ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              DATA INGESTION LAYER                               │
│                                                                                 │
│   ┌─────────────────┐      ┌─────────────────┐      ┌─────────────────┐        │
│   │   Price Feed    │      │   Order Book    │      │ Market Tracker  │        │
│   │  src/feed/      │      │  src/orderbook/ │      │  src/market/    │        │
│   │                 │      │                 │      │                 │        │
│   │  • binance.rs   │      │  • client.rs    │      │  • gamma.rs     │        │
│   │  • types.rs     │      │  • book.rs      │      │  • tracker.rs   │        │
│   │                 │      │                 │      │                 │        │
│   │  Emits:         │      │  Emits:         │      │  Provides:      │        │
│   │  PriceTick      │      │  OrderBook      │      │  Vec<Market>    │        │
│   └────────┬────────┘      └────────┬────────┘      └────────┬────────┘        │
│            │                        │                        │                  │
└────────────┼────────────────────────┼────────────────────────┼──────────────────┘
             │                        │                        │
             │                        │                        │
             ▼                        │                        │
┌─────────────────────────────────────┼────────────────────────┼──────────────────┐
│                         ANALYTICS LAYER                      │                  │
│                                                              │                  │
│   ┌─────────────────────────────────┐                        │                  │
│   │      Fair Value Model           │◄───────────────────────┘                  │
│   │      src/model/                 │                                           │
│   │                                 │    Uses market.open_price as strike (K)   │
│   │  • gbm.rs        ┌──────────────┴──────────────┐                           │
│   │  • volatility.rs │  GBM Probability Model      │                           │
│   │                  │                             │                           │
│   │                  │  P(up) = N(d2)              │                           │
│   │                  │  d2 = (ln(S/K) - 0.5σ²T)   │                           │
│   │                  │       ─────────────────     │                           │
│   │                  │          σ√T               │                           │
│   │                  │                             │                           │
│   │                  │  S = spot price (Binance)   │                           │
│   │                  │  K = open price (market)    │                           │
│   │                  │  σ = rolling volatility     │                           │
│   │                  │  T = time to expiry         │                           │
│   │                  └──────────────┬──────────────┘                           │
│   │                                 │                                           │
│   │  Output: FairValue {            │                                           │
│   │    yes_prob, no_prob,           │                                           │
│   │    confidence                   │                                           │
│   │  }                              │                                           │
│   └─────────────────────────────────┘                                           │
│                    │                                                            │
└────────────────────┼────────────────────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                            SIGNAL LAYER                                         │
│                                                                                 │
│   ┌─────────────────────────────────────────────────────────────────────┐      │
│   │                    Signal Generator                                  │      │
│   │                    src/signal/                                       │      │
│   │                                                                      │      │
│   │   ┌──────────────┐    ┌──────────────┐    ┌──────────────┐         │      │
│   │   │  detector.rs │───▶│  filter.rs   │───▶│   types.rs   │         │      │
│   │   └──────────────┘    └──────────────┘    └──────────────┘         │      │
│   │                                                                      │      │
│   │   Edge Calculation:        Filter Chain:          Output Signal:     │      │
│   │   ─────────────────       ─────────────────      ─────────────────  │      │
│   │   raw_edge =              • min_edge (0.5%)      • market           │      │
│   │     fair_value -          • max_edge (10%)       • side (Yes/No)    │      │
│   │     market_price          • min_liquidity        • fair_value       │      │
│   │                           • min_time_expiry      • market_price     │      │
│   │   adjusted_edge =         • max_time_expiry      • edge             │      │
│   │     raw_edge -            • volatility_sanity    • confidence       │      │
│   │     fees -                                       • reason           │      │
│   │     slippage                                                        │      │
│   │                                                                      │      │
│   └──────────────────────────────────────────────────────────────────────┘      │
│                    │                                                            │
│                    │ Signal (if edge passes all filters)                        │
│                    ▼                                                            │
└─────────────────────────────────────────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         RISK & EXECUTION LAYER                                  │
│                                                                                 │
│   ┌───────────────────────────────┐    ┌───────────────────────────────┐       │
│   │       Risk Manager            │    │     Execution Engine          │       │
│   │       src/risk/               │    │     src/execution/            │       │
│   │                               │    │                               │       │
│   │  ┌─────────┐ ┌─────────┐     │    │  ┌─────────────────────────┐ │       │
│   │  │kelly.rs │ │limits.rs│     │    │  │     paper.rs            │ │       │
│   │  └────┬────┘ └────┬────┘     │    │  │   (MVP: Paper Trading)  │ │       │
│   │       │           │          │    │  │                         │ │       │
│   │       ▼           ▼          │    │  │  Simulated execution    │ │       │
│   │  ┌──────────────────────┐   │    │  │  with realistic fills   │ │       │
│   │  │   Position Sizing    │   │    │  └─────────────────────────┘ │       │
│   │  │                      │   │    │                               │       │
│   │  │  Kelly: f* = 0.25x   │   │◀───┼───────────────────────────────┘       │
│   │  │  Max: 1% bankroll    │   │    │                                        │
│   │  │  Max positions: 3    │   │    │  ┌─────────────────────────┐          │
│   │  └──────────────────────┘   │    │  │     types.rs            │          │
│   │       │                      │    │  │  Order, Fill, Side     │          │
│   │       ▼                      │    │  └─────────────────────────┘          │
│   │  ┌──────────────────────┐   │    │                                        │
│   │  │  position.rs         │   │    │                                        │
│   │  │  PositionTracker     │   │    └────────────────────────────────────────┘
│   │  │  • open_positions    │   │                                              │
│   │  │  • total_exposure    │   │                                              │
│   │  │  • unrealized_pnl    │   │                                              │
│   │  └──────────────────────┘   │                                              │
│   │                              │                                              │
│   │  Drawdown Controls:          │                                              │
│   │  • Max daily loss: 5%        │                                              │
│   │  • Max drawdown: 10%         │                                              │
│   │  • Max exposure: 10%         │                                              │
│   │                              │                                              │
│   └──────────────────────────────┘                                              │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                          PERSISTENCE LAYER                                      │
│                                                                                 │
│   ┌───────────────────────────────┐    ┌───────────────────────────────┐       │
│   │      Data Capture             │    │       Backtester              │       │
│   │      src/data/                │    │       src/backtest/           │       │
│   │                               │    │                               │       │
│   │  • recorder.rs                │    │  • replay.rs (event stream)   │       │
│   │  • parquet.rs                 │    │  • simulator.rs               │       │
│   │                               │    │  • execution_model.rs         │       │
│   │  Writes:                      │    │  • analytics.rs               │       │
│   │  ┌───────────────────────┐   │    │                               │       │
│   │  │ price_ticks.parquet   │   │◀───│  Reads captured data,         │       │
│   │  │ orderbook_ticks.pqt   │   │    │  replays events, simulates    │       │
│   │  │ signals.parquet       │   │    │  queue position & fills       │       │
│   │  └───────────────────────┘   │    │                               │       │
│   │                               │    │  Outputs:                     │       │
│   │  Rotation: hourly             │    │  • Sharpe, Sortino ratios     │       │
│   │                               │    │  • Win rate, profit factor    │       │
│   │                               │    │  • Equity curve               │       │
│   │                               │    │  • Trade log                  │       │
│   └───────────────────────────────┘    └───────────────────────────────┘       │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘


┌─────────────────────────────────────────────────────────────────────────────────┐
│                        CROSS-CUTTING CONCERNS                                   │
│                                                                                 │
│   ┌───────────────────────────────┐    ┌───────────────────────────────┐       │
│   │      CLI Interface            │    │      Observability            │       │
│   │      src/cli/                 │    │      src/telemetry/           │       │
│   │                               │    │                               │       │
│   │  Commands:                    │    │  • metrics.rs (Prometheus)    │       │
│   │  • poly-hft run               │    │  • tracing.rs (OpenTelemetry) │       │
│   │  • poly-hft capture           │    │  • logging.rs (JSON logs)     │       │
│   │  • poly-hft backtest          │    │                               │       │
│   │  • poly-hft status            │    │  Metrics:                     │       │
│   │  • poly-hft config            │    │  • Latency histograms         │       │
│   │                               │    │  • Trade counters             │       │
│   │  Built with clap derive       │    │  • P&L gauges                 │       │
│   │                               │    │  • Drawdown tracking          │       │
│   └───────────────────────────────┘    └───────────────────────────────┘       │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Data Flow Sequence

```
                    MAIN TRADING LOOP (< 100ms target)
═══════════════════════════════════════════════════════════════════════════════

  Time ─────────────────────────────────────────────────────────────────────►

  1. PRICE UPDATE                    2. CALCULATE              3. GENERATE
     (< 50ms latency)                   FAIR VALUE                SIGNAL
                                        (< 10ms)                  (< 10ms)

  ┌─────────┐   PriceTick    ┌─────────────┐  FairValue   ┌─────────────┐
  │ Binance │ ─────────────► │  Fair Value │ ───────────► │   Signal    │
  │   WS    │                │    Model    │              │  Generator  │
  └─────────┘                └─────────────┘              └──────┬──────┘
                                    ▲                            │
                                    │                            │ Signal
                                    │ Market (open_price, expiry)│ (if edge
                             ┌──────┴──────┐                     │  detected)
                             │   Market    │                     │
                             │   Tracker   │                     │
                             └──────┬──────┘                     │
                                    ▲                            │
                                    │ Market Discovery           │
                             ┌──────┴──────┐                     │
                             │  Gamma API  │                     │
                             │  (30s poll) │                     │
                             └─────────────┘                     │
                                                                 │
                                                                 ▼
  4. RISK CHECK              5. SIZE POSITION          6. EXECUTE ORDER

  ┌─────────────┐  Pass/Fail  ┌─────────────┐  Order   ┌─────────────┐
  │    Risk     │ ◄────────── │   Kelly     │ ◄─────── │  Execution  │
  │   Limits    │             │   Sizing    │          │   Engine    │
  └─────────────┘             └─────────────┘          └──────┬──────┘
                                                              │
                                                              │ Fill
                                                              ▼
                                                       ┌─────────────┐
                                                       │  Position   │
                                                       │   Tracker   │
                                                       └─────────────┘

═══════════════════════════════════════════════════════════════════════════════
```

## Component Interactions

### Real-Time Data Flow (Hot Path)

```
                              ┌─────────────────────────────────────┐
                              │          HOT PATH (< 100ms)         │
                              └─────────────────────────────────────┘

     ┌──────────────────────────────────────────────────────────────────┐
     │                                                                  │
     │   Binance WS ──► Price Feed ──► Fair Value ──► Signal Gen ──►  │
     │       │              │              │              │             │
     │       │              ▼              │              ▼             │
     │       │         Data Capture       │         Risk Check ──►    │
     │       │              │              │              │             │
     │       │              ▼              │              ▼             │
     │       │      price_ticks.pqt       │      Execution Engine      │
     │       │                            │              │             │
     │       └────────────────────────────┴──────────────┴─────────────┘
     │                                                                  │
     │   Polymarket WS ──► Order Book ──────────────────┐              │
     │       │                  │                        │              │
     │       │                  ▼                        ▼              │
     │       │          orderbook_ticks.pqt     (feeds into Signal     │
     │       │                                   Generator for         │
     │       │                                   liquidity & pricing)  │
     │       │                                                         │
     └──────────────────────────────────────────────────────────────────┘
```

### Background Processes (Cold Path)

```
                              ┌─────────────────────────────────────┐
                              │         COLD PATH (periodic)        │
                              └─────────────────────────────────────┘

     ┌──────────────────────────────────────────────────────────────────┐
     │                                                                  │
     │   Market Discovery (every 30s)                                   │
     │   ┌──────────┐      ┌──────────────┐      ┌──────────────┐      │
     │   │ Gamma API│ ───► │Market Tracker│ ───► │ Active Markets│      │
     │   └──────────┘      └──────────────┘      └──────────────┘      │
     │                                                                  │
     │   Data Rotation (every 1h)                                       │
     │   ┌──────────┐      ┌──────────────┐                            │
     │   │ Recorder │ ───► │ New Parquet  │                            │
     │   └──────────┘      │    File      │                            │
     │                     └──────────────┘                            │
     │                                                                  │
     │   Metrics Export (continuous)                                    │
     │   ┌──────────┐      ┌──────────────┐                            │
     │   │Telemetry │ ───► │ Prometheus   │                            │
     │   └──────────┘      │   :9090      │                            │
     │                     └──────────────┘                            │
     │                                                                  │
     └──────────────────────────────────────────────────────────────────┘
```

## Module Dependency Graph

```
                              src/lib.rs
                                  │
           ┌──────────────────────┼──────────────────────┐
           │                      │                      │
           ▼                      ▼                      ▼
      ┌─────────┐           ┌──────────┐           ┌──────────┐
      │  feed/  │           │ market/  │           │orderbook/│
      │         │           │          │           │          │
      │Binance  │           │Gamma API │           │Polymarket│
      │   WS    │           │Discovery │           │Order Book│
      └────┬────┘           └────┬─────┘           └────┬─────┘
           │                     │                      │
           │                     │                      │
           └──────────┬──────────┴──────────────────────┘
                      │
                      ▼
                 ┌─────────┐
                 │ model/  │
                 │         │
                 │GBM Fair │
                 │ Value   │
                 └────┬────┘
                      │
                      ▼
                 ┌─────────┐
                 │ signal/ │
                 │         │
                 │Detector │
                 │& Filter │
                 └────┬────┘
                      │
           ┌──────────┴──────────┐
           │                     │
           ▼                     ▼
      ┌─────────┐           ┌─────────┐
      │  risk/  │           │execution│
      │         │◄──────────│         │
      │ Kelly & │           │ Paper   │
      │Position │           │ Engine  │
      └────┬────┘           └────┬────┘
           │                     │
           └──────────┬──────────┘
                      │
                      ▼
                 ┌─────────┐
                 │  data/  │
                 │         │
                 │Parquet  │
                 │Capture  │
                 └────┬────┘
                      │
                      ▼
                 ┌─────────┐
                 │backtest/│
                 │         │
                 │Replay & │
                 │Simulate │
                 └─────────┘

                      │
      Cross-cutting:  │
                      ▼
     ┌────────────────┴────────────────┐
     │                                 │
     ▼                                 ▼
┌─────────┐                       ┌─────────┐
│  cli/   │                       │telemetry│
│         │                       │         │
│Commands │                       │Metrics &│
│(clap)   │                       │Tracing  │
└─────────┘                       └─────────┘
```

## Key Traits (Interfaces)

```
┌──────────────────────────────────────────────────────────────────────┐
│                         TRAIT ABSTRACTIONS                           │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  PriceFeed (src/feed/)                                               │
│  ├── subscribe() -> Receiver<PriceTick>                              │
│  └── Implementations: BinanceFeed                                    │
│                                                                      │
│  MarketTracker (src/market/)                                         │
│  ├── get_active_markets() -> Vec<Market>                             │
│  ├── refresh()                                                       │
│  └── Implementations: GammaMarketTracker                             │
│                                                                      │
│  FairValueModel (src/model/)                                         │
│  ├── calculate(params) -> FairValue                                  │
│  └── Implementations: GbmModel                                       │
│                                                                      │
│  ExecutionEngine (src/execution/)                                    │
│  ├── submit_order(order) -> OrderId                                  │
│  ├── cancel_order(id)                                                │
│  ├── get_fills() -> Vec<Fill>                                        │
│  └── Implementations: PaperEngine, (future: LiveEngine)              │
│                                                                      │
│  RiskManager (src/risk/)                                             │
│  ├── calculate_size(signal, bankroll) -> Decimal                     │
│  ├── check_limits(order, tracker) -> Result                          │
│  ├── should_halt() -> Option<HaltReason>                             │
│  └── Implementations: DefaultRiskManager                             │
│                                                                      │
│  Backtester (src/backtest/)                                          │
│  ├── run(config) -> BacktestResult                                   │
│  └── Implementations: EventDrivenBacktester                          │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

---

## Component Descriptions

### 1. Data Ingestion Layer

**Price Feed** (`src/feed/`)
- Connects to Binance WebSocket (`btcusdt@trade` stream)
- Converts raw trade events into `PriceTick` structs with timestamps
- Provides async channel-based subscription for downstream consumers
- Target latency: < 50ms from exchange to bot

**Order Book** (`src/orderbook/`)
- Maintains L2 order book state from Polymarket WebSocket
- Tracks best bid/ask and depth for YES and NO tokens
- Used by Signal Generator to assess liquidity and current market prices

**Market Tracker** (`src/market/`)
- Polls Gamma API every 30 seconds to discover active 15-minute BTC up/down markets
- Tracks market open price (strike), open time, and expiry time
- Handles market rollovers when one 15-min window closes and another opens

### 2. Analytics Layer

**Fair Value Model** (`src/model/`)
- Implements Geometric Brownian Motion (GBM) probability model
- Calculates P(BTC will be up at expiry) using Black-Scholes-style formula
- Inputs: spot price (Binance), open price (market), time-to-expiry, volatility
- Volatility estimated from rolling window of recent price returns

### 3. Signal Layer

**Signal Generator** (`src/signal/`)
- Compares fair value to current Polymarket prices
- Calculates edge: `fair_value - market_price - fees - slippage`
- Applies filter chain: minimum edge, maximum edge, liquidity, time-to-expiry, volatility bounds
- Outputs `Signal` with market, side, edge, confidence, and reason

### 4. Risk & Execution Layer

**Risk Manager** (`src/risk/`)
- **Kelly Sizing**: Calculates optimal position size using quarter-Kelly (0.25x)
- **Position Limits**: Max 1% of bankroll per trade, max 3 concurrent positions
- **Drawdown Controls**: Halts trading if daily loss > 5% or drawdown > 10%
- **Position Tracking**: Maintains open positions, calculates unrealized P&L

**Execution Engine** (`src/execution/`)
- MVP: Paper trading with simulated fills
- Future: Live execution against Polymarket CLOB
- Tracks order state and fill events

### 5. Persistence Layer

**Data Capture** (`src/data/`)
- Records all price ticks, order book snapshots, and signals to Parquet files
- Hourly file rotation for manageability
- Enables offline backtesting

**Backtester** (`src/backtest/`)
- Event-driven replay of historical Parquet data
- Queue position simulation for realistic fill modeling
- Outputs: Sharpe ratio, win rate, equity curve, trade log

### 6. Cross-Cutting Concerns

**CLI** (`src/cli/`)
- `run`: Start paper trading
- `capture`: Data capture only (no trading)
- `backtest`: Run strategy on historical data
- `status`: Show current state
- `config`: View/edit configuration

**Observability** (`src/telemetry/`)
- **Prometheus Metrics**: Latency histograms, trade counters, P&L gauges
- **OpenTelemetry Tracing**: Distributed traces for the trading loop
- **Structured Logging**: JSON logs with span context

---

## Execution Modes

```
┌────────────────────────────────────────────────────────────────────────────────┐
│                                                                                │
│                         poly-hft run (Paper Trading)                           │
│   ┌──────────────────────────────────────────────────────────────────────┐    │
│   │  Live data feeds ──► Full strategy logic ──► Paper execution         │    │
│   │  (Binance + Polymarket WS)                   (simulated fills)       │    │
│   └──────────────────────────────────────────────────────────────────────┘    │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│                                                                                │
│                         poly-hft capture (Data Only)                           │
│   ┌──────────────────────────────────────────────────────────────────────┐    │
│   │  Live data feeds ──► Record to Parquet                               │    │
│   │  (no signal generation, no trading)                                  │    │
│   └──────────────────────────────────────────────────────────────────────┘    │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│                                                                                │
│                         poly-hft backtest (Historical)                         │
│   ┌──────────────────────────────────────────────────────────────────────┐    │
│   │  Parquet files ──► Event replay ──► Strategy ──► Simulated queue     │    │
│   │  (offline)         (time-ordered)   (same logic)  (fill modeling)    │    │
│   └──────────────────────────────────────────────────────────────────────┘    │
│                                                                                │
└────────────────────────────────────────────────────────────────────────────────┘
```
