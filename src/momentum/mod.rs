//! Momentum detection module
//!
//! Detects significant price momentum from a strike price using a rolling window.
//! This is the foundation of the lag edges strategy: detect momentum FIRST,
//! then check if Polymarket odds are lagging.

mod detector;
mod types;

pub use detector::{MomentumConfig, MomentumDetector};
pub use types::{MomentumDirection, MomentumSignal};
