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
}
