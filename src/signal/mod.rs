//! Signal generation module
//!
//! Detects tradeable pricing discrepancies

mod detector;
mod filter;
mod momentum_detector;
mod types;

pub use detector::SignalDetector;
pub use filter::{FilterResult, RejectReason, SignalFilter};
pub use momentum_detector::{DetectionResult, MomentumSignalDetector};
pub use types::{Side, Signal, SignalReason};
