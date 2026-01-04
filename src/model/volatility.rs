//! Volatility estimation module
//!
//! Rolling realized volatility from price returns

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use std::collections::VecDeque;

/// Rolling volatility estimator from log returns
pub struct VolatilityEstimator {
    /// Window duration for volatility calculation
    window: Duration,
    /// Price history with timestamps
    prices: VecDeque<(DateTime<Utc>, Decimal)>,
}

impl VolatilityEstimator {
    /// Create a new volatility estimator with given window
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            prices: VecDeque::new(),
        }
    }

    /// Add a new price observation
    pub fn update(&mut self, timestamp: DateTime<Utc>, price: Decimal) {
        // Add new price
        self.prices.push_back((timestamp, price));

        // Remove old prices outside window
        let cutoff = timestamp - self.window;
        while let Some((ts, _)) = self.prices.front() {
            if *ts < cutoff {
                self.prices.pop_front();
            } else {
                break;
            }
        }
    }

    /// Calculate annualized realized volatility
    pub fn estimate(&self) -> Option<Decimal> {
        if self.prices.len() < 2 {
            return None;
        }

        // Calculate log returns
        let mut returns: Vec<f64> = Vec::new();
        for i in 1..self.prices.len() {
            let prev_price: f64 = self.prices[i - 1].1.try_into().unwrap_or(0.0);
            let curr_price: f64 = self.prices[i].1.try_into().unwrap_or(0.0);
            if prev_price > 0.0 && curr_price > 0.0 {
                returns.push((curr_price / prev_price).ln());
            }
        }

        if returns.is_empty() {
            return None;
        }

        // Calculate standard deviation of returns
        let n = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / n;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
        let std_dev = variance.sqrt();

        // Annualize: assume ~1 tick per second, so sqrt(seconds_per_year)
        // seconds per year ≈ 31,536,000
        let avg_interval = self.window.num_seconds() as f64 / n;
        let intervals_per_year = 31_536_000.0 / avg_interval;
        let annualized = std_dev * intervals_per_year.sqrt();

        Decimal::try_from(annualized).ok()
    }

    /// Get standard error of volatility estimate
    pub fn standard_error(&self) -> Option<Decimal> {
        let vol = self.estimate()?;
        let n = self.prices.len();
        if n < 2 {
            return None;
        }
        // SE ≈ vol / sqrt(2n)
        let se: f64 = f64::try_from(vol).unwrap_or(0.0) / (2.0 * n as f64).sqrt();
        Decimal::try_from(se).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_volatility_estimator() {
        let mut estimator = VolatilityEstimator::new(Duration::minutes(30));

        let base_time = Utc::now();
        let prices = vec![
            dec!(100000),
            dec!(100010),
            dec!(99990),
            dec!(100020),
            dec!(99980),
        ];

        for (i, price) in prices.into_iter().enumerate() {
            let ts = base_time + Duration::seconds(i as i64);
            estimator.update(ts, price);
        }

        let vol = estimator.estimate();
        assert!(vol.is_some());
        assert!(vol.unwrap() > dec!(0));
    }

    #[test]
    fn test_volatility_estimator_new() {
        let estimator = VolatilityEstimator::new(Duration::minutes(5));
        assert!(estimator.estimate().is_none()); // Empty estimator
    }

    #[test]
    fn test_volatility_single_price() {
        let mut estimator = VolatilityEstimator::new(Duration::minutes(5));
        estimator.update(Utc::now(), dec!(100000));
        // Single price cannot calculate volatility
        assert!(estimator.estimate().is_none());
    }

    #[test]
    fn test_volatility_two_prices() {
        let mut estimator = VolatilityEstimator::new(Duration::minutes(5));
        let base_time = Utc::now();
        estimator.update(base_time, dec!(100000));
        estimator.update(base_time + Duration::seconds(1), dec!(100100));
        // Two prices can calculate volatility
        assert!(estimator.estimate().is_some());
    }

    #[test]
    fn test_volatility_window_expiry() {
        let mut estimator = VolatilityEstimator::new(Duration::seconds(5));
        let base_time = Utc::now();

        // Add prices within window
        estimator.update(base_time, dec!(100000));
        estimator.update(base_time + Duration::seconds(1), dec!(100100));
        estimator.update(base_time + Duration::seconds(2), dec!(100200));

        // Add prices that will expire old ones but keep at least 2
        estimator.update(base_time + Duration::seconds(6), dec!(100300));
        estimator.update(base_time + Duration::seconds(7), dec!(100400));

        // Should still have enough data points for volatility calculation
        let vol = estimator.estimate();
        assert!(vol.is_some());
    }

    #[test]
    fn test_volatility_constant_price() {
        let mut estimator = VolatilityEstimator::new(Duration::minutes(5));
        let base_time = Utc::now();

        // Add same price multiple times
        for i in 0..5 {
            estimator.update(base_time + Duration::seconds(i), dec!(100000));
        }

        let vol = estimator.estimate();
        // Constant price should have zero or near-zero volatility
        assert!(vol.is_some());
        assert!(vol.unwrap() < dec!(0.001)); // Very low volatility
    }

    #[test]
    fn test_standard_error() {
        let mut estimator = VolatilityEstimator::new(Duration::minutes(30));
        let base_time = Utc::now();

        for i in 0..10 {
            let price = dec!(100000) + Decimal::from(i * 10);
            estimator.update(base_time + Duration::seconds(i), price);
        }

        let se = estimator.standard_error();
        assert!(se.is_some());
        assert!(se.unwrap() > dec!(0));
    }

    #[test]
    fn test_standard_error_insufficient_data() {
        let estimator = VolatilityEstimator::new(Duration::minutes(5));
        // No data
        assert!(estimator.standard_error().is_none());
    }

    #[test]
    fn test_standard_error_single_price() {
        let mut estimator = VolatilityEstimator::new(Duration::minutes(5));
        estimator.update(Utc::now(), dec!(100000));
        // Single price cannot calculate standard error
        assert!(estimator.standard_error().is_none());
    }

    #[test]
    fn test_volatility_increasing_prices() {
        let mut estimator = VolatilityEstimator::new(Duration::minutes(30));
        let base_time = Utc::now();

        // Steadily increasing prices
        for i in 0..20 {
            let price = dec!(100000) + Decimal::from(i * 100);
            estimator.update(base_time + Duration::seconds(i), price);
        }

        let vol = estimator.estimate();
        assert!(vol.is_some());
        assert!(vol.unwrap() > dec!(0));
    }

    #[test]
    fn test_volatility_decreasing_prices() {
        let mut estimator = VolatilityEstimator::new(Duration::minutes(30));
        let base_time = Utc::now();

        // Steadily decreasing prices
        for i in 0..20 {
            let price = dec!(100000) - Decimal::from(i * 100);
            estimator.update(base_time + Duration::seconds(i), price);
        }

        let vol = estimator.estimate();
        assert!(vol.is_some());
        assert!(vol.unwrap() > dec!(0));
    }
}
