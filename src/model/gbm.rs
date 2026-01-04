//! Geometric Brownian Motion fair value model
//!
//! Uses Black-Scholes-style probability calculation:
//! P(up) = N(d2) where d2 = (ln(S/K) - 0.5*sigma^2*T) / (sigma*sqrt(T))

use super::{FairValue, FairValueModel, FairValueParams};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// GBM-based fair value model
pub struct GbmModel;

impl GbmModel {
    /// Create a new GBM model
    pub fn new() -> Self {
        Self
    }
}

impl Default for GbmModel {
    fn default() -> Self {
        Self::new()
    }
}

impl FairValueModel for GbmModel {
    fn calculate(&self, params: FairValueParams) -> FairValue {
        // Convert time to expiry to years
        let t_secs = params.time_to_expiry.num_seconds() as f64;
        let t_years = t_secs / (365.25 * 24.0 * 60.0 * 60.0);

        if t_years <= 0.0 || params.volatility == dec!(0) {
            // At expiry or zero vol: deterministic outcome
            let yes_prob = if params.current_price >= params.open_price {
                dec!(1)
            } else {
                dec!(0)
            };
            return FairValue {
                yes_prob,
                no_prob: Decimal::ONE - yes_prob,
                confidence: dec!(1),
            };
        }

        // Calculate d2 = (ln(S/K) - 0.5*sigma^2*T) / (sigma*sqrt(T))
        let s: f64 = params.current_price.try_into().unwrap_or(0.0);
        let k: f64 = params.open_price.try_into().unwrap_or(0.0);
        let sigma: f64 = params.volatility.try_into().unwrap_or(0.0);

        if k <= 0.0 || s <= 0.0 {
            return FairValue {
                yes_prob: dec!(0.5),
                no_prob: dec!(0.5),
                confidence: dec!(0),
            };
        }

        let d2 = ((s / k).ln() - 0.5 * sigma * sigma * t_years) / (sigma * t_years.sqrt());

        // N(d2) using standard normal CDF approximation
        let yes_prob_f64 = normal_cdf(d2);
        let yes_prob = Decimal::try_from(yes_prob_f64).unwrap_or(dec!(0.5));
        let no_prob = Decimal::ONE - yes_prob;

        // Confidence based on time to expiry (higher confidence closer to expiry)
        let confidence = Decimal::try_from(1.0 - t_years.min(1.0)).unwrap_or(dec!(0.5));

        FairValue {
            yes_prob,
            no_prob,
            confidence,
        }
    }
}

/// Standard normal CDF approximation (Abramowitz and Stegun)
fn normal_cdf(x: f64) -> f64 {
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs() / std::f64::consts::SQRT_2;

    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();

    0.5 * (1.0 + sign * y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_gbm_at_the_money() {
        let model = GbmModel::new();
        let params = FairValueParams {
            current_price: dec!(100000),
            open_price: dec!(100000),
            time_to_expiry: Duration::minutes(7),
            volatility: dec!(0.50), // 50% annualized
        };

        let fair_value = model.calculate(params);
        // At the money should be close to 50%
        assert!(fair_value.yes_prob > dec!(0.45) && fair_value.yes_prob < dec!(0.55));
    }

    #[test]
    fn test_gbm_in_the_money() {
        let model = GbmModel::new();
        let params = FairValueParams {
            current_price: dec!(101000),
            open_price: dec!(100000),
            time_to_expiry: Duration::minutes(1),
            volatility: dec!(0.50),
        };

        let fair_value = model.calculate(params);
        // Price 1% above open with 1 min left should favor Yes
        assert!(fair_value.yes_prob > dec!(0.6));
    }
}
