//! Lag detection module
//!
//! Detects when Polymarket odds are lagging behind confirmed spot price momentum.
//! This is the core of the lag edges strategy:
//!
//! 1. Momentum detection confirms BTC has moved significantly from strike
//! 2. Check if Polymarket odds are still in neutral zone (40-60 cents)
//! 3. If odds haven't caught up, there's a lag to exploit
//! 4. Generate trade signal to buy YES (up momentum) or NO (down momentum)

mod detector;
mod types;

pub use detector::{LagDetector, LagDetectorConfig};
pub use types::{LagSignal, NoLagReason, OddsState, TradeSide};
