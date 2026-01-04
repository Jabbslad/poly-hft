//! Backtest analytics and reporting

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::path::PathBuf;

/// Summary statistics from backtest
#[derive(Debug, Clone, Default)]
pub struct BacktestSummary {
    /// Total P&L
    pub total_pnl: Decimal,
    /// Net P&L after fees
    pub net_pnl: Decimal,
    /// Sharpe ratio
    pub sharpe_ratio: Decimal,
    /// Sortino ratio
    pub sortino_ratio: Decimal,
    /// Win rate percentage
    pub win_rate: Decimal,
    /// Profit factor
    pub profit_factor: Decimal,
    /// Maximum drawdown (absolute)
    pub max_drawdown: Decimal,
    /// Maximum drawdown (percentage)
    pub max_drawdown_pct: Decimal,
    /// Total number of trades
    pub total_trades: usize,
    /// Average trade duration in seconds
    pub avg_trade_duration_secs: u64,
    /// Average edge captured
    pub avg_edge: Decimal,
}

/// Complete backtest results
#[derive(Debug, Clone)]
pub struct BacktestResult {
    /// Summary statistics
    pub summary: BacktestSummary,
    /// Path to trades Parquet file
    pub trades_path: PathBuf,
    /// Path to equity curve Parquet file
    pub equity_path: PathBuf,
}

impl Default for BacktestResult {
    fn default() -> Self {
        Self {
            summary: BacktestSummary::default(),
            trades_path: PathBuf::from("backtest_trades.parquet"),
            equity_path: PathBuf::from("equity_curve.parquet"),
        }
    }
}

impl BacktestSummary {
    /// Format as table for CLI output
    pub fn format_table(&self) -> String {
        format!(
            r#"
══════════════════════════════════════════════════════
               BACKTEST RESULTS
══════════════════════════════════════════════════════

PERFORMANCE
───────────────────────────────────────────────────────
Net P&L:          {:+.2} ({:+.2}%)
Sharpe Ratio:     {:.2}
Sortino Ratio:    {:.2}
Max Drawdown:     {:.2} ({:.2}%)
Win Rate:         {:.1}%
Profit Factor:    {:.2}

ACTIVITY
───────────────────────────────────────────────────────
Total Trades:     {}
Avg Duration:     {}s
Avg Edge:         {:.2}%
══════════════════════════════════════════════════════
"#,
            self.net_pnl,
            self.net_pnl * dec!(100),
            self.sharpe_ratio,
            self.sortino_ratio,
            self.max_drawdown,
            self.max_drawdown_pct * dec!(100),
            self.win_rate * dec!(100),
            self.profit_factor,
            self.total_trades,
            self.avg_trade_duration_secs,
            self.avg_edge * dec!(100),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backtest_summary_default() {
        let summary = BacktestSummary::default();
        assert_eq!(summary.total_pnl, dec!(0));
        assert_eq!(summary.net_pnl, dec!(0));
        assert_eq!(summary.total_trades, 0);
    }

    #[test]
    fn test_backtest_summary_clone() {
        let mut summary = BacktestSummary::default();
        summary.total_pnl = dec!(100);
        summary.net_pnl = dec!(95);
        summary.sharpe_ratio = dec!(1.5);
        summary.win_rate = dec!(0.65);
        summary.total_trades = 50;

        let cloned = summary.clone();
        assert_eq!(cloned.total_pnl, dec!(100));
        assert_eq!(cloned.net_pnl, dec!(95));
        assert_eq!(cloned.sharpe_ratio, dec!(1.5));
        assert_eq!(cloned.win_rate, dec!(0.65));
        assert_eq!(cloned.total_trades, 50);
    }

    #[test]
    fn test_backtest_summary_format_table() {
        let summary = BacktestSummary {
            total_pnl: dec!(100),
            net_pnl: dec!(95),
            sharpe_ratio: dec!(1.5),
            sortino_ratio: dec!(2.0),
            win_rate: dec!(0.65),
            profit_factor: dec!(1.8),
            max_drawdown: dec!(10),
            max_drawdown_pct: dec!(0.05),
            total_trades: 50,
            avg_trade_duration_secs: 300,
            avg_edge: dec!(0.02),
        };

        let table = summary.format_table();
        assert!(table.contains("BACKTEST RESULTS"));
        assert!(table.contains("Net P&L"));
        assert!(table.contains("Sharpe Ratio"));
        assert!(table.contains("Total Trades"));
    }

    #[test]
    fn test_backtest_result_default() {
        let result = BacktestResult::default();
        assert_eq!(result.summary.total_pnl, dec!(0));
        assert!(result
            .trades_path
            .to_string_lossy()
            .contains("backtest_trades"));
        assert!(result
            .equity_path
            .to_string_lossy()
            .contains("equity_curve"));
    }

    #[test]
    fn test_backtest_result_clone() {
        let result = BacktestResult::default();
        let cloned = result.clone();
        assert_eq!(result.trades_path, cloned.trades_path);
        assert_eq!(result.equity_path, cloned.equity_path);
    }
}
