//! Paper trading execution engine

use super::{ExecutionEngine, Fill, Order, OrderId};
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Paper trading execution engine with simulated fills
pub struct PaperEngine {
    fee_rate: Decimal,
    fills: Arc<RwLock<Vec<Fill>>>,
}

impl PaperEngine {
    /// Create a new paper trading engine
    pub fn new(fee_rate: Decimal) -> Self {
        Self {
            fee_rate,
            fills: Arc::new(RwLock::new(vec![])),
        }
    }
}

#[async_trait]
impl ExecutionEngine for PaperEngine {
    async fn submit_order(&self, order: Order) -> anyhow::Result<OrderId> {
        let order_id = OrderId::new_v4();

        // Simulate immediate fill at order price
        let fees = order.size * order.price * self.fee_rate;
        let fill = Fill {
            order_id,
            token_id: order.token_id,
            side: order.side,
            price: order.price,
            size: order.size,
            timestamp: Utc::now(),
            fees,
        };

        let mut fills = self.fills.write().await;
        fills.push(fill);

        tracing::info!(?order_id, "Paper order filled");
        Ok(order_id)
    }

    async fn cancel_order(&self, id: OrderId) -> anyhow::Result<()> {
        tracing::info!(?id, "Paper order cancelled");
        Ok(())
    }

    async fn get_fills(&self) -> anyhow::Result<Vec<Fill>> {
        let fills = self.fills.read().await;
        Ok(fills.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::OrderType;
    use crate::signal::Side;
    use rust_decimal_macros::dec;

    #[tokio::test]
    async fn test_paper_engine_fill() {
        let engine = PaperEngine::new(dec!(0.001));

        let order = Order {
            token_id: "test".to_string(),
            side: Side::Yes,
            price: dec!(0.50),
            size: dec!(100),
            order_type: OrderType::Limit,
        };

        let order_id = engine.submit_order(order).await.unwrap();
        let fills = engine.get_fills().await.unwrap();

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].order_id, order_id);
        assert_eq!(fills[0].fees, dec!(0.05)); // 100 * 0.50 * 0.001
    }

    #[tokio::test]
    async fn test_paper_engine_cancel() {
        let engine = PaperEngine::new(dec!(0.001));
        let order_id = OrderId::new_v4();

        // Cancel should succeed (no-op in paper trading)
        let result = engine.cancel_order(order_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_paper_engine_multiple_orders() {
        let engine = PaperEngine::new(dec!(0.002));

        let order1 = Order {
            token_id: "yes-token".to_string(),
            side: Side::Yes,
            price: dec!(0.55),
            size: dec!(50),
            order_type: OrderType::Market,
        };

        let order2 = Order {
            token_id: "no-token".to_string(),
            side: Side::No,
            price: dec!(0.45),
            size: dec!(75),
            order_type: OrderType::Limit,
        };

        engine.submit_order(order1).await.unwrap();
        engine.submit_order(order2).await.unwrap();

        let fills = engine.get_fills().await.unwrap();
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[0].side, Side::Yes);
        assert_eq!(fills[1].side, Side::No);
    }

    #[tokio::test]
    async fn test_paper_engine_zero_fee() {
        let engine = PaperEngine::new(dec!(0));

        let order = Order {
            token_id: "test".to_string(),
            side: Side::Yes,
            price: dec!(0.50),
            size: dec!(100),
            order_type: OrderType::Limit,
        };

        engine.submit_order(order).await.unwrap();
        let fills = engine.get_fills().await.unwrap();

        assert_eq!(fills[0].fees, dec!(0));
    }
}
