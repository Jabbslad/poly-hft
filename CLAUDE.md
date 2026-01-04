# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

poly-hft is a high-frequency trading bot for Polymarket's 15-minute BTC up/down binary markets. It exploits pricing lags between real-time Binance spot prices and Polymarket odds.

**IMPORTANT**: All implementation work MUST adhere to the PRD at `docs/PRD.md`. This document defines the architecture, module interfaces, quality standards, and implementation phases. Do not deviate from the PRD without explicit user approval.

## Build Commands

```bash
cargo build              # Build the project
cargo build --release    # Build with optimizations
cargo test               # Run all tests
cargo test <name>        # Run tests matching <name>
cargo test -- --nocapture  # Run tests with stdout visible
cargo clippy -- -D warnings  # Run linter (fails on warnings)
cargo fmt --check        # Check formatting without modifying
```

## Pre-commit Hooks

Uses `cargo-husky` for automatic pre-commit checks. Hooks install automatically on first `cargo build`/`cargo test`. Runs: `cargo fmt`, `cargo clippy`, `cargo test`.

## CLI Commands

```bash
poly-hft run          # Start paper trading
poly-hft capture      # Data capture only (no trading)
poly-hft backtest     # Run backtest on captured data
poly-hft status       # Show current state
poly-hft config       # Show/edit configuration
```

## Architecture

**Data Flow**: Binance WS → Price Engine → Fair Value Model → Signal Generator → Execution Engine

**Key Concepts**:
- **Fair Value Model** (`src/model/gbm.rs`): Uses GBM to calculate P(up) = N(d2) based on spot price vs market open price
- **Signal Filters** (`src/signal/filter.rs`): Edge thresholds, liquidity checks, volatility sanity, time-to-expiry limits
- **Kelly Sizing** (`src/risk/kelly.rs`): Quarter Kelly (0.25x) with 1% max position cap
- **Queue Simulation** (`src/backtest/execution_model.rs`): Models order book queue position for realistic backtesting

**Critical Types**:
- Use `rust_decimal::Decimal` for all prices/sizes (never f64)
- Use `chrono::DateTime<Utc>` for all timestamps
- Traits define module interfaces (e.g., `PriceFeed`, `ExecutionEngine`, `RiskManager`)

## Quality Standards

- Test coverage: >= 75%
- Clippy clean, rustfmt formatted
- Price feed latency < 50ms, signal generation < 10ms

## Documentation

- `docs/PRD.md` - Product Requirements Document (authoritative)
