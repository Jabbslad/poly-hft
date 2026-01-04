# poly-hft Trading Flow

## Overview

This document describes the complete trading flow for poly-hft, a high-frequency trading bot that exploits pricing lags between real-time Binance BTC prices and Polymarket's 15-minute up/down binary markets.

---

## The Core Insight

```
Polymarket 15-minute BTC markets work like this:

  Market opens at 12:00:00
  Open price (strike) = $95,000
  Question: "Will BTC be above $95,000 at 12:15:00?"

  YES token pays $1.00 if BTC > $95,000 at expiry
  NO token pays $1.00 if BTC ≤ $95,000 at expiry

The EDGE we exploit:

  When BTC moves on Binance, Polymarket odds take time to update.

  12:00:00 - Market opens, BTC = $95,000, YES = $0.50 (fair)
  12:01:23 - BTC spikes to $95,800 on Binance
  12:01:24 - Fair value of YES is now ~$0.72 (calculated via GBM model)
  12:01:24 - Polymarket YES still showing $0.52 (stale!)

  Edge = $0.72 - $0.52 = $0.20 (20 cents per share!)
```

---

## Complete System Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           POLY-HFT TRADING LOOP                             │
│                         Target: < 100ms end-to-end                          │
└─────────────────────────────────────────────────────────────────────────────┘

     PHASE 1: DATA INGESTION (Continuous)
     ════════════════════════════════════

     ┌──────────────┐         ┌──────────────┐         ┌──────────────┐
     │   BINANCE    │         │  POLYMARKET  │         │  GAMMA API   │
     │   WebSocket  │         │   WebSocket  │         │   (REST)     │
     │              │         │              │         │              │
     │ BTC/USDT     │         │ Order Book   │         │ 15-min BTC   │
     │ Trade Stream │         │ Updates      │         │ Markets      │
     └──────┬───────┘         └──────┬───────┘         └──────┬───────┘
            │ <50ms                  │ <50ms                  │ 30s poll
            ▼                        ▼                        ▼
     ┌──────────────┐         ┌──────────────┐         ┌──────────────┐
     │ Price Feed   │         │ Order Book   │         │ Market       │
     │              │         │ Aggregator   │         │ Tracker      │
     │ • Latest BTC │         │ • Best bid   │         │ • Open price │
     │   spot price │         │ • Best ask   │         │ • Expiry     │
     │ • Volatility │         │ • Depth      │         │ • Token IDs  │
     │   (rolling)  │         │ • Liquidity  │         │              │
     └──────┬───────┘         └──────┬───────┘         └──────┬───────┘
            │                        │                        │
            └────────────────────────┼────────────────────────┘
                                     │
                                     ▼

     PHASE 2: FAIR VALUE CALCULATION (<10ms)
     ═══════════════════════════════════════

                        ┌─────────────────────────┐
                        │    FAIR VALUE MODEL     │
                        │    (GBM / Black-Scholes)│
                        └─────────────────────────┘
                                     │
              ┌──────────────────────┼──────────────────────┐
              │                      │                      │
              ▼                      ▼                      ▼
     ┌─────────────────┐   ┌─────────────────┐   ┌─────────────────┐
     │ INPUTS          │   │ FORMULA         │   │ OUTPUT          │
     │                 │   │                 │   │                 │
     │ S = $95,800     │   │ d2 = ln(S/K) -  │   │ P(up) = 0.72    │
     │ (current spot)  │   │      0.5σ²T     │   │ P(down) = 0.28  │
     │                 │   │      ───────    │   │                 │
     │ K = $95,000     │   │       σ√T       │   │ Confidence: 0.85│
     │ (open/strike)   │   │                 │   │                 │
     │                 │   │ P(up) = N(d2)   │   │                 │
     │ T = 13.6 min    │   │                 │   │                 │
     │ (time to expiry)│   │ N() = normal CDF│   │                 │
     │                 │   │                 │   │                 │
     │ σ = 45% annual  │   │                 │   │                 │
     │ (volatility)    │   │                 │   │                 │
     └─────────────────┘   └─────────────────┘   └─────────────────┘
                                     │
                                     ▼

     PHASE 3: SIGNAL DETECTION (<10ms)
     ══════════════════════════════════

                        ┌─────────────────────────┐
                        │    SIGNAL GENERATOR     │
                        └─────────────────────────┘
                                     │
                                     ▼
     ┌───────────────────────────────────────────────────────────────┐
     │                      EDGE CALCULATION                         │
     │                                                               │
     │   Fair Value (YES):     $0.72                                 │
     │   Market Price (YES):   $0.52  (best ask on Polymarket)       │
     │   ─────────────────────────────                               │
     │   Raw Edge:             $0.20  (20%)                          │
     │                                                               │
     │   Estimated Fees:       $0.005 (0.5% taker fee)               │
     │   Estimated Slippage:   $0.005 (0.5% for our size)            │
     │   Delay Decay Buffer:   $0.015 (1.5% for 500ms taker delay)   │
     │   ─────────────────────────────                               │
     │   Adjusted Edge:        $0.175 (17.5%)                        │
     │                                                               │
     └───────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
     ┌───────────────────────────────────────────────────────────────┐
     │                       FILTER CHAIN                            │
     │                    (ALL must pass)                            │
     │                                                               │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Edge ≥ 0.5%        Adjusted edge 17.5% ≥ 0.5%     │    │
     │   └─────────────────────────────────────────────────────┘    │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Edge ≤ 10%         17.5% > 10%? NO → SUSPICIOUS   │    │
     │   │                      (But raw 20% ok, decay-adjusted)│    │
     │   └─────────────────────────────────────────────────────┘    │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Time to expiry     13.6 min > 1 min minimum       │    │
     │   │   ≥ 60 seconds       13.6 min < 14 min maximum      │    │
     │   └─────────────────────────────────────────────────────┘    │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Liquidity          $500 available at $0.52        │    │
     │   │   ≥ order size       Our order: $50 ✓               │    │
     │   └─────────────────────────────────────────────────────┘    │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Volatility         45% annual is reasonable       │    │
     │   │   10% < σ < 200%     (not broken data)              │    │
     │   └─────────────────────────────────────────────────────┘    │
     │                                                               │
     │   RESULT: ✅ SIGNAL GENERATED                                 │
     │                                                               │
     └───────────────────────────────────────────────────────────────┘
                                     │
                                     ▼

     PHASE 4: ORDER TYPE DECISION
     ════════════════════════════

     ┌───────────────────────────────────────────────────────────────┐
     │                   MAKER vs TAKER DECISION                     │
     │                                                               │
     │   Adjusted Edge = 17.5%                                       │
     │                                                               │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │  IF edge ≥ 3% (HIGH_THRESHOLD):                     │    │
     │   │     → Use TAKER order                               │    │
     │   │     → Edge large enough to survive 500ms delay      │    │
     │   │     → Guaranteed fill (if liquidity exists)         │    │
     │   │                                                     │    │
     │   │  17.5% ≥ 3%  →  ✅ USE TAKER ORDER                  │    │
     │   └─────────────────────────────────────────────────────┘    │
     │                                                               │
     │   Alternative paths (not taken this time):                    │
     │                                                               │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │  IF 1% ≤ edge < 3% (MEDIUM_THRESHOLD):              │    │
     │   │     → Use MAKER order (post-only)                   │    │
     │   │     → Post at (best_bid + $0.001)                   │    │
     │   │     → No delay, but may not fill                    │    │
     │   └─────────────────────────────────────────────────────┘    │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │  IF edge < 1% (LOW_THRESHOLD):                      │    │
     │   │     → NO TRADE                                      │    │
     │   │     → Edge too small to justify risk                │    │
     │   └─────────────────────────────────────────────────────┘    │
     │                                                               │
     └───────────────────────────────────────────────────────────────┘
                                     │
                                     ▼

     PHASE 5: POSITION SIZING (Kelly Criterion)
     ══════════════════════════════════════════

     ┌───────────────────────────────────────────────────────────────┐
     │                      KELLY SIZING                             │
     │                                                               │
     │   Bankroll:           $500                                    │
     │   Fair value (p):     0.72                                    │
     │   Market price:       0.52                                    │
     │   Payout if win:      $1.00                                   │
     │                                                               │
     │   Odds (b) = (1 - market_price) / market_price                │
     │           = (1 - 0.52) / 0.52 = 0.923                         │
     │                                                               │
     │   Full Kelly: f* = (p × b - q) / b                            │
     │                  = (0.72 × 0.923 - 0.28) / 0.923              │
     │                  = 0.416 (41.6% of bankroll!)                 │
     │                                                               │
     │   Quarter Kelly: f = 0.416 × 0.25 = 0.104 (10.4%)             │
     │                                                               │
     │   Uncapped size: $500 × 10.4% = $52                           │
     │                                                               │
     │   Hard cap (1% max): $500 × 1% = $5                           │
     │                                                               │
     │   FINAL SIZE: min($52, $5) = $5.00                            │
     │                                                               │
     └───────────────────────────────────────────────────────────────┘
                                     │
                                     ▼

     PHASE 6: RISK CHECK
     ═══════════════════

     ┌───────────────────────────────────────────────────────────────┐
     │                      RISK LIMITS                              │
     │                                                               │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Position size      $5 ≤ 1% of $500 bankroll       │    │
     │   └─────────────────────────────────────────────────────┘    │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Concurrent pos.    1 open < 3 max                 │    │
     │   └─────────────────────────────────────────────────────┘    │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Total exposure     $5 + $3 existing = $8          │    │
     │   │                      $8 < 10% of $500 = $50 ✓       │    │
     │   └─────────────────────────────────────────────────────┘    │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Daily P&L          +$12 today (not at -5% limit)  │    │
     │   └─────────────────────────────────────────────────────┘    │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Drawdown           -2% from peak (not at -10%)    │    │
     │   └─────────────────────────────────────────────────────┘    │
     │                                                               │
     │   RESULT: ✅ RISK CHECK PASSED                                │
     │                                                               │
     └───────────────────────────────────────────────────────────────┘
                                     │
                                     ▼

     PHASE 7: ORDER EXECUTION
     ════════════════════════

     ┌───────────────────────────────────────────────────────────────┐
     │                    ORDER SUBMISSION                           │
     │                                                               │
     │   Order Type:    LIMIT (taker, marketable)                    │
     │   Side:          BUY                                          │
     │   Token:         YES (token_id: 0x1234...)                    │
     │   Price:         $0.52                                        │
     │   Size:          9.6 shares ($5.00 / $0.52)                   │
     │                                                               │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │              EXECUTION TIMELINE                     │    │
     │   │                                                     │    │
     │   │  T+0ms      Order signed (Rust SDK, <100ms)         │    │
     │   │  T+100ms    Order submitted to Polymarket CLOB      │    │
     │   │  T+100ms    ┌──────────────────────────────────┐   │    │
     │   │             │      500ms TAKER DELAY QUEUE      │   │    │
     │   │             │                                    │   │    │
     │   │             │  Our order waits here while        │   │    │
     │   │             │  market makers can update quotes   │   │    │
     │   │             │                                    │   │    │
     │   │  T+600ms    └──────────────────────────────────┘   │    │
     │   │  T+600ms    Order matched against best offer        │    │
     │   │  T+650ms    Fill confirmed                          │    │
     │   │  T+700ms    Settlement queued on-chain              │    │
     │   │                                                     │    │
     │   └─────────────────────────────────────────────────────┘    │
     │                                                               │
     │   RESULT: FILLED 9.6 shares @ $0.52 = $4.99                   │
     │                                                               │
     └───────────────────────────────────────────────────────────────┘
                                     │
                                     ▼

     PHASE 8: POSITION TRACKING
     ══════════════════════════

     ┌───────────────────────────────────────────────────────────────┐
     │                    POSITION OPENED                            │
     │                                                               │
     │   Position ID:     pos_abc123                                 │
     │   Market:          BTC 15-min Up/Down (12:00-12:15)           │
     │   Side:            YES                                        │
     │   Entry Price:     $0.52                                      │
     │   Shares:          9.6                                        │
     │   Entry Time:      12:01:24                                   │
     │   Expiry:          12:15:00                                   │
     │   Cost Basis:      $4.99                                      │
     │                                                               │
     │   If BTC > $95,000 at 12:15:00:                               │
     │     Payout = 9.6 × $1.00 = $9.60                              │
     │     Profit = $9.60 - $4.99 = $4.61 (92% return)               │
     │                                                               │
     │   If BTC ≤ $95,000 at 12:15:00:                               │
     │     Payout = $0.00                                            │
     │     Loss = -$4.99 (100% loss on position)                     │
     │                                                               │
     │   Expected Value (at entry):                                  │
     │     EV = (0.72 × $9.60) + (0.28 × $0) - $4.99                 │
     │        = $6.91 - $4.99 = +$1.92 expected profit               │
     │                                                               │
     └───────────────────────────────────────────────────────────────┘
                                     │
                                     ▼

     PHASE 9: POSITION EXIT (at settlement)
     ══════════════════════════════════════

     ┌───────────────────────────────────────────────────────────────┐
     │                    MARKET SETTLEMENT                          │
     │                                                               │
     │   Time: 12:15:00                                              │
     │   BTC Final Price: $95,423 (from Chainlink oracle)            │
     │   Result: BTC > $95,000 → YES wins                            │
     │                                                               │
     │   Position Resolution:                                        │
     │     YES shares:    9.6                                        │
     │     Payout:        9.6 × $1.00 = $9.60                        │
     │     Cost basis:    $4.99                                      │
     │     Gross profit:  $9.60 - $4.99 = $4.61                      │
     │     Fees paid:     $0.05 (taker fee)                          │
     │     Net profit:    $4.56 (91.4% return)                       │
     │                                                               │
     │   Bankroll Update:                                            │
     │     Before:        $500.00                                    │
     │     After:         $504.56                                    │
     │                                                               │
     └───────────────────────────────────────────────────────────────┘
```

---

## Alternative Flow: Maker Order Path

When edge is smaller (1-3%), we use maker orders to avoid the delay:

```
     MAKER ORDER FLOW (edge = 1.5%)
     ═══════════════════════════════

     ┌───────────────────────────────────────────────────────────────┐
     │                                                               │
     │   Fair Value:      $0.55                                      │
     │   Best Ask:        $0.52                                      │
     │   Best Bid:        $0.48                                      │
     │   Edge:            $0.03 (3% raw, ~1.5% after costs)          │
     │                                                               │
     │   Decision: Edge too small for taker delay → USE MAKER        │
     │                                                               │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │              POST MAKER ORDER                        │    │
     │   │                                                      │    │
     │   │   Post BUY at $0.49 (above best bid, below ask)      │    │
     │   │                                                      │    │
     │   │   Order Book Before:     Order Book After:           │    │
     │   │   ─────────────────     ─────────────────            │    │
     │   │   ASK  $0.52  100       ASK  $0.52  100              │    │
     │   │   ASK  $0.51   50       ASK  $0.51   50              │    │
     │   │                         ▶BID  $0.49   10◀ (OUR ORDER)│    │
     │   │   BID  $0.48   80       BID  $0.48   80              │    │
     │   │   BID  $0.47  120       BID  $0.47  120              │    │
     │   │                                                      │    │
     │   └─────────────────────────────────────────────────────┘    │
     │                                                               │
     │   Possible Outcomes:                                          │
     │                                                               │
     │   A) FILLED: Someone sells into our $0.49 bid                 │
     │      → We buy at $0.49 (better than $0.52!)                   │
     │      → No taker delay applied to us                           │
     │      → Effective edge: 6% ($0.55 - $0.49)                     │
     │                                                               │
     │   B) NOT FILLED: Price moves away                             │
     │      → After 500ms, cancel order                              │
     │      → No loss, no position                                   │
     │      → Missed opportunity, but no damage                      │
     │                                                               │
     │   C) PARTIAL FILL: Get some shares before price moves         │
     │      → Smaller position than intended                         │
     │      → Still profitable on filled portion                     │
     │                                                               │
     └───────────────────────────────────────────────────────────────┘
```

---

## Complete State Machine

```
                              ┌──────────────┐
                              │    IDLE      │
                              │  (waiting)   │
                              └──────┬───────┘
                                     │
                          Price tick or book update
                                     │
                                     ▼
                              ┌──────────────┐
                              │  CALCULATE   │
                              │  FAIR VALUE  │
                              └──────┬───────┘
                                     │
                                     ▼
                              ┌──────────────┐
                          ┌───│   DETECT     │───┐
                          │   │    EDGE      │   │
                          │   └──────────────┘   │
                          │                      │
                    No edge                 Edge found
                          │                      │
                          ▼                      ▼
                   ┌──────────────┐       ┌──────────────┐
                   │    IDLE      │       │   FILTER     │
                   │  (waiting)   │       │   SIGNAL     │
                   └──────────────┘       └──────┬───────┘
                                                 │
                                    ┌────────────┴────────────┐
                                    │                         │
                              Filters pass              Filters fail
                                    │                         │
                                    ▼                         ▼
                             ┌──────────────┐          ┌──────────────┐
                             │ CHOOSE ORDER │          │    IDLE      │
                             │    TYPE      │          │ (log reject) │
                             └──────┬───────┘          └──────────────┘
                                    │
                       ┌────────────┴────────────┐
                       │                         │
                  Edge ≥ 3%                 1% ≤ Edge < 3%
                       │                         │
                       ▼                         ▼
                ┌──────────────┐          ┌──────────────┐
                │ TAKER ORDER  │          │ MAKER ORDER  │
                │  (immediate) │          │  (post-only) │
                └──────┬───────┘          └──────┬───────┘
                       │                         │
                       ▼                         ▼
                ┌──────────────┐          ┌──────────────┐
                │  500ms DELAY │          │  WAIT FOR    │
                │    QUEUE     │          │    FILL      │
                └──────┬───────┘          └──────┬───────┘
                       │                         │
                       │                ┌────────┴────────┐
                       │                │                 │
                       │            Filled           Timeout/Cancel
                       │                │                 │
                       ▼                ▼                 ▼
                ┌──────────────┐ ┌──────────────┐  ┌──────────────┐
                │   FILLED     │ │   FILLED     │  │    IDLE      │
                │              │ │              │  │  (no fill)   │
                └──────┬───────┘ └──────┬───────┘  └──────────────┘
                       │                │
                       └────────┬───────┘
                                │
                                ▼
                         ┌──────────────┐
                         │   POSITION   │
                         │    OPEN      │
                         └──────┬───────┘
                                │
                   ┌────────────┴────────────┐
                   │                         │
            Market settles            Edge disappears
                   │                    (optional exit)
                   │                         │
                   ▼                         ▼
            ┌──────────────┐          ┌──────────────┐
            │  SETTLEMENT  │          │  EARLY EXIT  │
            │  (auto-pay)  │          │ (sell shares)│
            └──────┬───────┘          └──────┬───────┘
                   │                         │
                   └────────────┬────────────┘
                                │
                                ▼
                         ┌──────────────┐
                         │   UPDATE     │
                         │   BANKROLL   │
                         └──────┬───────┘
                                │
                                ▼
                         ┌──────────────┐
                         │    IDLE      │
                         │  (next tick) │
                         └──────────────┘
```

---

## Timing Budget

```
┌─────────────────────────────────────────────────────────────────────┐
│                    END-TO-END LATENCY BUDGET                        │
│                        Target: < 100ms                              │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│   Component                    Budget        Actual (target)        │
│   ─────────────────────────────────────────────────────────         │
│   Binance WS → Price Feed      < 50ms        ~30ms                  │
│   Fair Value Calculation       < 10ms        ~2ms                   │
│   Signal Generation            < 10ms        ~3ms                   │
│   Risk Check                   < 5ms         ~1ms                   │
│   Order Signing (Rust)         < 20ms        ~10ms                  │
│   Network to Polymarket        < 20ms        ~15ms                  │
│   ─────────────────────────────────────────────────────────         │
│   TOTAL (our control)          < 100ms       ~61ms ✓                │
│                                                                     │
│   + Taker Delay (not our control)  ~500ms                           │
│   ─────────────────────────────────────────────────────────         │
│   TOTAL with delay             ~600ms                               │
│                                                                     │
│   This is why edge must be large enough to survive 500ms decay      │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Summary

| Phase | Action | Time Budget |
|-------|--------|-------------|
| 1 | Data ingestion (continuous) | < 50ms latency |
| 2 | Fair value calculation | < 10ms |
| 3 | Signal detection + filtering | < 10ms |
| 4 | Order type decision | < 1ms |
| 5 | Kelly position sizing | < 1ms |
| 6 | Risk limit checks | < 5ms |
| 7 | Order execution | < 20ms + 500ms delay |
| 8 | Position tracking | < 1ms |
| 9 | Settlement | Automatic at expiry |

**Key insight**: Our speed (100ms) matters for getting into the queue early. The 500ms taker delay is unavoidable, so we compensate by requiring larger edges when taking.
