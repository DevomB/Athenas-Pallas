//! Bar interval helpers for Sharpe/Sortino annualization.

use crate::instrument::AssetClass;

/// Map a resample interval label to periods per year (crypto-style 24/7 unless noted).
pub fn periods_per_year_from_interval(interval: &str) -> Option<f64> {
    match interval.trim().to_lowercase().as_str() {
        "1s" => Some(31_536_000.0),
        "1m" => Some(525_600.0),
        "3m" => Some(175_200.0),
        "5m" => Some(105_120.0),
        "15m" => Some(35_040.0),
        "30m" => Some(17_520.0),
        "1h" | "60m" => Some(8_760.0),
        "2h" => Some(4_380.0),
        "4h" => Some(2_190.0),
        "6h" => Some(1_460.0),
        "8h" => Some(1_095.0),
        "12h" => Some(730.0),
        "3d" => Some(122.0),
        "1wk" | "1w" => Some(52.0),
        "1mo" => Some(12.0),
        "1d" => None,
        _ => None,
    }
}

/// Resolve periods/year using interval string and asset class (daily bars use 252 for equities).
pub fn periods_per_year_from_interval_for_class(interval: &str, class: AssetClass) -> f64 {
    if let Some(p) = periods_per_year_from_interval(interval) {
        return p;
    }
    if interval.trim().eq_ignore_ascii_case("1d") {
        return match class {
            AssetClass::Equity => 252.0,
            _ => 365.0,
        };
    }
    default_periods_per_year(class)
}

/// Default annualization when interval is unknown.
pub fn default_periods_per_year(class: AssetClass) -> f64 {
    match class {
        AssetClass::Equity => 252.0,
        _ => 365.0,
    }
}

/// Infer periods/year from median bar spacing in seconds.
pub fn infer_periods_per_year_from_spacing(median_secs: f64, class: AssetClass) -> f64 {
    if median_secs <= 0.0 || !median_secs.is_finite() {
        return default_periods_per_year(class);
    }
    let trading_secs_per_year = trading_seconds_per_year(class);
    (trading_secs_per_year / median_secs).max(1.0)
}

fn trading_seconds_per_year(class: AssetClass) -> f64 {
    match class {
        AssetClass::Equity => 252.0 * 6.5 * 3_600.0,
        AssetClass::Forex => 365.0 * 24.0 * 3_600.0 * (5.0 / 7.0),
        _ => 365.0 * 24.0 * 3_600.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_equity_uses_252() {
        assert_eq!(
            periods_per_year_from_interval_for_class("1d", AssetClass::Equity),
            252.0
        );
    }

    #[test]
    fn hourly_crypto() {
        assert_eq!(periods_per_year_from_interval("1h"), Some(8_760.0));
    }

    #[test]
    fn infer_from_one_day_spacing() {
        let ppy = infer_periods_per_year_from_spacing(86_400.0, AssetClass::Crypto);
        assert!((ppy - 365.0).abs() < 0.01);
    }
}
