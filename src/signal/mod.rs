//! Signal generation module
//!
//! Detects tradeable pricing discrepancies

mod detector;
mod filter;
mod spread;
mod spread_orchestrator;
mod types;

pub use detector::SignalDetector;
pub use filter::{FilterResult, RejectReason, SignalFilter};
pub use spread::{MarketBooks, SpreadConfig, SpreadDetector, SpreadSignal};
pub use spread_orchestrator::SpreadOrchestrator;
pub use types::{Side, Signal, SignalReason};
