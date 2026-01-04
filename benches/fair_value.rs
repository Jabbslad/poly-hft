//! Benchmarks for fair value calculation

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use poly_hft::model::{FairValueModel, FairValueParams, GbmModel};
use rust_decimal_macros::dec;

fn benchmark_gbm_calculation(c: &mut Criterion) {
    let model = GbmModel::new();

    let params = FairValueParams {
        current_price: dec!(100500),
        open_price: dec!(100000),
        time_to_expiry: chrono::Duration::minutes(7),
        volatility: dec!(0.50),
    };

    c.bench_function("gbm_fair_value", |b| {
        b.iter(|| model.calculate(black_box(params.clone())))
    });
}

fn benchmark_gbm_at_the_money(c: &mut Criterion) {
    let model = GbmModel::new();

    let params = FairValueParams {
        current_price: dec!(100000),
        open_price: dec!(100000),
        time_to_expiry: chrono::Duration::minutes(7),
        volatility: dec!(0.50),
    };

    c.bench_function("gbm_fair_value_atm", |b| {
        b.iter(|| model.calculate(black_box(params.clone())))
    });
}

criterion_group!(
    benches,
    benchmark_gbm_calculation,
    benchmark_gbm_at_the_money
);
criterion_main!(benches);
