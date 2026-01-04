//! Execution engine module
//!
//! Handles order submission (paper and live modes)

mod paper;
mod types;

pub use paper::PaperEngine;
pub use types::{Fill, Order, OrderId, OrderType};

use async_trait::async_trait;

/// Trait for execution engine implementations
#[async_trait]
pub trait ExecutionEngine: Send + Sync {
    /// Submit an order
    async fn submit_order(&self, order: Order) -> anyhow::Result<OrderId>;
    /// Cancel an order
    async fn cancel_order(&self, id: OrderId) -> anyhow::Result<()>;
    /// Get all fills
    async fn get_fills(&self) -> anyhow::Result<Vec<Fill>>;
}
