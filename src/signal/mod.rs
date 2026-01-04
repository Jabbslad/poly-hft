//! Signal generation module
//!
//! Detects tradeable pricing discrepancies

mod detector;
mod filter;
mod types;

pub use detector::SignalDetector;
pub use filter::{FilterResult, RejectReason, SignalFilter};
pub use types::{Side, Signal, SignalReason};
