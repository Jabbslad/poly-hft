# poly-hft Trading Flow: Lag Edges Strategy

## Overview

This document describes the complete trading flow for poly-hft, a trading bot that exploits pricing lags between real-time Binance BTC prices and Polymarket's 15-minute up/down binary markets using the **momentum-first lag detection** approach.

---

## The Core Insight: Momentum First, Then Check Odds

```
Polymarket 15-minute BTC markets work like this:

  Market opens at 12:00:00
  Strike price = $95,000 (BTC price at market open)
  Question: "Will BTC be above $95,000 at 12:15:00?"

  YES token pays $1.00 if BTC > $95,000 at expiry
  NO token pays $1.00 if BTC ≤ $95,000 at expiry

The LAG EDGE we exploit:

  1. DETECT MOMENTUM FIRST: Watch Binance for significant moves from strike
  2. CHECK IF ODDS LAG: Are Polymarket odds still in the "neutral zone"?
  3. SIGNAL WHEN BOTH: Momentum confirmed AND odds haven't caught up

  Timeline Example:
  12:00:00 - Market opens, BTC = $95,000 (strike), YES = $0.50 (fair)
             → No edge yet (BTC at strike, odds are correct)

  12:02:30 - BTC at $95,750 (+0.79% momentum detected!)
             → Polymarket YES still at $0.52 (LAGGING!)
             → SIGNAL: Buy YES (momentum UP, odds still neutral)

  12:03:00 - Polymarket catches up, YES now $0.68
             → Edge gone (too late)

KEY INSIGHT: Edge only exists DURING the market window, not at open.
```

## Critical Timing Window

```
┌────────────────────────────────────────────────────────────────────────┐
│                     15-MINUTE MARKET LIFECYCLE                          │
├────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  00:00 ─────── 01:00 ─────── 05:00 ─────── 12:00 ─────── 13:00 ── 15:00│
│    │             │             │             │             │        │  │
│    │   NO EDGE   │  EDGE       │   PRIME     │  EDGE       │  NO    │  │
│    │   (at open) │  WINDOW     │   WINDOW    │  SHRINKS    │  EDGE  │  │
│    │             │             │             │             │        │  │
│    ▼             ▼             ▼             ▼             ▼        ▼  │
│  Strike=BTC   Momentum      Larger moves   Odds catch    Too late     │
│  Odds=50/50   can form      more likely    up gradually  to trade     │
│                                                                         │
└────────────────────────────────────────────────────────────────────────┘

At Market Open (00:00):
  - Strike = current BTC price
  - YES = 50¢, NO = 50¢ (fair odds)
  - NO EDGE (nothing to exploit)

1-5 Minutes After Open:
  - BTC may have moved from strike
  - Polymarket odds may still be ~50¢
  - EDGE WINDOW (lag exists if momentum + neutral odds)

5-12 Minutes After Open:
  - PRIME WINDOW for larger moves
  - Still enough time for position to pay off
  - Best risk/reward

12-15 Minutes (Near Close):
  - Odds have had time to adjust
  - Less opportunity for lag
  - EDGE SHRINKS
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

     PHASE 2: MOMENTUM DETECTION (<10ms)
     ════════════════════════════════════

                        ┌─────────────────────────┐
                        │   MOMENTUM DETECTOR     │
                        │   (Rolling 2-min window)│
                        └─────────────────────────┘
                                     │
              ┌──────────────────────┼──────────────────────┐
              │                      │                      │
              ▼                      ▼                      ▼
     ┌─────────────────┐   ┌─────────────────┐   ┌─────────────────┐
     │ INPUTS          │   │ CALCULATION     │   │ OUTPUT          │
     │                 │   │                 │   │                 │
     │ Current price:  │   │ momentum =      │   │ Direction: UP   │
     │   $95,750       │   │ (current-strike)│   │ Move: +0.79%    │
     │                 │   │ ─────────────── │   │ Confirmed: YES  │
     │ Strike price:   │   │     strike      │   │                 │
     │   $95,000       │   │                 │   │                 │
     │                 │   │ = ($95,750 -    │   │                 │
     │ Time since open:│   │    $95,000)     │   │                 │
     │   2 min 30 sec  │   │   / $95,000     │   │                 │
     │                 │   │                 │   │                 │
     │ Min threshold:  │   │ = +0.79%        │   │                 │
     │   0.7%          │   │   > 0.7% ✓      │   │                 │
     └─────────────────┘   └─────────────────┘   └─────────────────┘
                                     │
                                     ▼

     PHASE 3: LAG DETECTION (<10ms)
     ═══════════════════════════════

                        ┌─────────────────────────┐
                        │     LAG DETECTOR        │
                        │ (Momentum vs Odds Check)│
                        └─────────────────────────┘
                                     │
                                     ▼
     ┌───────────────────────────────────────────────────────────────┐
     │                      LAG DETECTION LOGIC                      │
     │                                                               │
     │   Momentum Direction:   UP (+0.79%)                           │
     │   Polymarket YES Price: $0.52  (from order book)              │
     │   ─────────────────────────────                               │
     │                                                               │
     │   CHECK: Is momentum UP but YES still in neutral zone?        │
     │                                                               │
     │   YES price $0.52 < $0.60 (max_yes_for_up threshold)          │
     │   → ODDS ARE LAGGING! ✓                                       │
     │                                                               │
     │   Lag magnitude: ~18 cents (should be ~$0.70, is $0.52)       │
     │   ─────────────────────────────                               │
     │   SIGNAL: BUY YES (momentum confirmed, odds lagging)          │
     │                                                               │
     └───────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
     ┌───────────────────────────────────────────────────────────────┐
     │                       FILTER CHAIN                            │
     │                    (ALL must pass)                            │
     │                                                               │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Time since open    2m 30s > 60s minimum           │    │
     │   │   ≥ 60 seconds       (avoid trading at market open)  │    │
     │   └─────────────────────────────────────────────────────┘    │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Time to close      12m 30s > 2 min minimum        │    │
     │   │   ≥ 120 seconds      (avoid trading near close)      │    │
     │   └─────────────────────────────────────────────────────┘    │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Momentum size      0.79% ≥ 0.7% threshold         │    │
     │   │   ≥ 0.7%             (significant move detected)     │    │
     │   └─────────────────────────────────────────────────────┘    │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Odds in neutral    YES = $0.52 < $0.60 threshold  │    │
     │   │   zone               (odds haven't caught up yet)    │    │
     │   └─────────────────────────────────────────────────────┘    │
     │   ┌─────────────────────────────────────────────────────┐    │
     │   │ ✓ Liquidity          $500 available at $0.52        │    │
     │   │   ≥ order size       Our order: $10 ✓                │    │
     │   └─────────────────────────────────────────────────────┘    │
     │                                                               │
     │   RESULT: ✅ LAG SIGNAL GENERATED                             │
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

     PHASE 5: POSITION SIZING (Fixed Size)
     ═══════════════════════════════════════

     ┌───────────────────────────────────────────────────────────────┐
     │                      FIXED SIZING                             │
     │                                                               │
     │   The lag edges strategy uses FIXED sizing for simplicity:    │
     │   - Same amount every trade (mechanical execution)            │
     │   - No edge-based adjustments                                 │
     │   - Scales with bankroll as % of capital                      │
     │                                                               │
     │   Bankroll:           $100                                    │
     │   Fixed percentage:   10%                                     │
     │   Max percentage:     20% (safety cap)                        │
     │                                                               │
     │   Base size: $100 × 10% = $10                                 │
     │                                                               │
     │   Safety check: $10 < $100 × 20% = $20 ✓                      │
     │                                                               │
     │   FINAL SIZE: $10.00                                          │
     │                                                               │
     │   ─────────────────────────────────────────────────────       │
     │                                                               │
     │   As bankroll grows:                                          │
     │     $100  → $10 per trade                                     │
     │     $500  → $50 per trade                                     │
     │     $1000 → $100 per trade                                    │
     │     $10K  → $1000 per trade                                   │
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
| 2 | Momentum detection | < 10ms |
| 3 | Lag detection + filtering | < 10ms |
| 4 | Order type decision | < 1ms |
| 5 | Fixed position sizing | < 1ms |
| 6 | Risk limit checks | < 5ms |
| 7 | Order execution | < 20ms + 500ms delay |
| 8 | Position tracking | < 1ms |
| 9 | Settlement | Automatic at 15-min expiry |

**Key Insights**:

1. **Edge exists DURING the window, not at open**: At market open, odds are fair (50/50). The edge appears 1-12 minutes into the window when BTC has moved but odds haven't caught up.

2. **Momentum first, then check odds**: Detect spot movement from strike first, then verify Polymarket odds are still in the neutral zone (40-60 cents).

3. **Timing matters**: Avoid the first minute (no momentum yet) and last 2 minutes (odds already adjusted). Prime window is 2-10 minutes after open.

4. **Mechanical execution**: Fixed sizing, same trade every time. No complex edge calculations - just momentum + lagging odds = trade.
