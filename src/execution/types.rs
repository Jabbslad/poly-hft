//! Execution types

use crate::signal::Side;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Order identifier
pub type OrderId = Uuid;

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    /// Market order (immediate execution)
    Market,
    /// Limit order (price specified)
    Limit,
}

/// An order to be submitted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    /// Token identifier
    pub token_id: String,
    /// Trade side
    pub side: Side,
    /// Order price (for limit orders)
    pub price: Decimal,
    /// Order size
    pub size: Decimal,
    /// Order type
    pub order_type: OrderType,
}

/// A fill (executed trade)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    /// Order ID
    pub order_id: OrderId,
    /// Token ID
    pub token_id: String,
    /// Trade side
    pub side: Side,
    /// Fill price
    pub price: Decimal,
    /// Fill size
    pub size: Decimal,
    /// Fill timestamp
    pub timestamp: DateTime<Utc>,
    /// Fees paid
    pub fees: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_order_type_market() {
        let order_type = OrderType::Market;
        assert_eq!(order_type, OrderType::Market);
    }

    #[test]
    fn test_order_type_limit() {
        let order_type = OrderType::Limit;
        assert_eq!(order_type, OrderType::Limit);
    }

    #[test]
    fn test_order_type_clone() {
        let order_type = OrderType::Market;
        let cloned = order_type;
        assert_eq!(order_type, cloned);
    }

    #[test]
    fn test_order_creation() {
        let order = Order {
            token_id: "yes-token".to_string(),
            side: Side::Yes,
            price: dec!(0.55),
            size: dec!(100),
            order_type: OrderType::Limit,
        };

        assert_eq!(order.token_id, "yes-token");
        assert_eq!(order.side, Side::Yes);
        assert_eq!(order.price, dec!(0.55));
        assert_eq!(order.size, dec!(100));
        assert_eq!(order.order_type, OrderType::Limit);
    }

    #[test]
    fn test_order_clone() {
        let order = Order {
            token_id: "yes-token".to_string(),
            side: Side::Yes,
            price: dec!(0.55),
            size: dec!(100),
            order_type: OrderType::Limit,
        };

        let cloned = order.clone();
        assert_eq!(order.token_id, cloned.token_id);
        assert_eq!(order.price, cloned.price);
    }

    #[test]
    fn test_fill_creation() {
        let fill = Fill {
            order_id: Uuid::new_v4(),
            token_id: "yes-token".to_string(),
            side: Side::Yes,
            price: dec!(0.55),
            size: dec!(100),
            timestamp: Utc::now(),
            fees: dec!(0.5),
        };

        assert_eq!(fill.token_id, "yes-token");
        assert_eq!(fill.side, Side::Yes);
        assert_eq!(fill.price, dec!(0.55));
        assert_eq!(fill.fees, dec!(0.5));
    }

    #[test]
    fn test_fill_clone() {
        let fill = Fill {
            order_id: Uuid::new_v4(),
            token_id: "yes-token".to_string(),
            side: Side::Yes,
            price: dec!(0.55),
            size: dec!(100),
            timestamp: Utc::now(),
            fees: dec!(0.5),
        };

        let cloned = fill.clone();
        assert_eq!(fill.order_id, cloned.order_id);
        assert_eq!(fill.price, cloned.price);
    }

    #[test]
    fn test_order_type_debug() {
        let order_type = OrderType::Market;
        let debug_str = format!("{:?}", order_type);
        assert!(debug_str.contains("Market"));
    }

    #[test]
    fn test_order_debug() {
        let order = Order {
            token_id: "test".to_string(),
            side: Side::Yes,
            price: dec!(0.50),
            size: dec!(10),
            order_type: OrderType::Market,
        };
        let debug_str = format!("{:?}", order);
        assert!(debug_str.contains("test"));
    }
}
