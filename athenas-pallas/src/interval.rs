//! Bar interval helpers for Sharpe/Sortino annualization.

use crate::instrument::AssetClass;

const DAY_SECS: f64 = 86_400.0;
const YEAR_SECS: f64 = 365.0 * DAY_SECS;

#[derive(Clone, Copy)]
enum IntervalBasis {
    Seconds(f64),
    TradingDays(f64),
    Fixed(f64),
}

const INTERVALS: &[(&[&str], IntervalBasis)] = &[
    (&["1s"], IntervalBasis::Seconds(1.0)),
    (&["1m"], IntervalBasis::Seconds(60.0)),
    (&["3m"], IntervalBasis::Seconds(180.0)),
    (&["5m"], IntervalBasis::Seconds(300.0)),
    (&["15m"], IntervalBasis::Seconds(900.0)),
    (&["30m"], IntervalBasis::Seconds(1_800.0)),
    (&["1h", "60m"], IntervalBasis::Seconds(3_600.0)),
    (&["2h"], IntervalBasis::Seconds(7_200.0)),
    (&["4h"], IntervalBasis::Seconds(14_400.0)),
    (&["6h"], IntervalBasis::Seconds(21_600.0)),
    (&["8h"], IntervalBasis::Seconds(28_800.0)),
    (&["12h"], IntervalBasis::Seconds(43_200.0)),
    (&["1d"], IntervalBasis::TradingDays(1.0)),
    (&["3d"], IntervalBasis::TradingDays(3.0)),
    (&["1wk", "1w"], IntervalBasis::Fixed(52.0)),
    (&["1mo"], IntervalBasis::Fixed(12.0)),
];

fn scale_periods(basis: IntervalBasis, class: AssetClass) -> f64 {
    match basis {
        IntervalBasis::Seconds(seconds) => trading_seconds_per_year(class) / seconds,
        IntervalBasis::TradingDays(days) => default_periods_per_year(class) / days,
        IntervalBasis::Fixed(periods) => periods,
    }
}

fn interval_basis(interval: &str) -> Option<IntervalBasis> {
    let interval = interval.trim();
    INTERVALS
        .iter()
        .find(|(labels, _)| {
            labels
                .iter()
                .any(|label| interval.eq_ignore_ascii_case(label))
        })
        .map(|(_, basis)| *basis)
}

/// Map a resample interval label to periods per year for a continuously traded market.
///
/// Daily bars return `None` because callers need the asset class to distinguish trading days from
/// calendar days.
pub fn periods_per_year_from_interval(interval: &str) -> Option<f64> {
    if interval.trim().eq_ignore_ascii_case("1d") {
        return None;
    }
    interval_basis(interval).map(|basis| scale_periods(basis, AssetClass::Crypto))
}

/// Resolve periods/year using both the interval label and asset class.
pub fn periods_per_year_from_interval_for_class(interval: &str, class: AssetClass) -> f64 {
    interval_basis(interval)
        .map(|basis| scale_periods(basis, class))
        .unwrap_or_else(|| default_periods_per_year(class))
}

/// Default annualization when interval is unknown.
pub fn default_periods_per_year(class: AssetClass) -> f64 {
    match class {
        AssetClass::Equity => 252.0,
        AssetClass::Forex => 260.0,
        _ => 365.0,
    }
}

/// Infer periods/year from median bar spacing in seconds.
pub fn infer_periods_per_year_from_spacing(median_secs: f64, class: AssetClass) -> f64 {
    if median_secs <= 0.0 || !median_secs.is_finite() {
        return default_periods_per_year(class);
    }
    // Daily timestamps encode sessions, while weekly/monthly timestamps encode calendar gaps.
    // Exchange-calendar-aware inference can replace this split if irregular bars need exact scaling.
    if median_secs >= 5.0 * DAY_SECS {
        return (YEAR_SECS / median_secs).max(1.0);
    }
    if median_secs >= 0.75 * DAY_SECS {
        return (default_periods_per_year(class) * DAY_SECS / median_secs).max(1.0);
    }
    (trading_seconds_per_year(class) / median_secs).max(1.0)
}

fn trading_seconds_per_year(class: AssetClass) -> f64 {
    match class {
        AssetClass::Equity => 252.0 * 6.5 * 3_600.0,
        AssetClass::Forex => default_periods_per_year(class) * DAY_SECS,
        _ => YEAR_SECS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_scaling_uses_asset_trading_hours() {
        assert_eq!(
            periods_per_year_from_interval_for_class("1d", AssetClass::Equity),
            252.0
        );
        assert_eq!(
            periods_per_year_from_interval_for_class("1H", AssetClass::Equity),
            1_638.0
        );
        assert_eq!(
            periods_per_year_from_interval_for_class("1h", AssetClass::Forex),
            6_240.0
        );
        assert_eq!(periods_per_year_from_interval("1h"), Some(8_760.0));
    }

    #[test]
    fn daily_spacing_uses_trading_days() {
        assert_eq!(
            infer_periods_per_year_from_spacing(DAY_SECS, AssetClass::Equity),
            252.0
        );
        assert_eq!(
            infer_periods_per_year_from_spacing(DAY_SECS, AssetClass::Forex),
            260.0
        );
        assert_eq!(
            infer_periods_per_year_from_spacing(DAY_SECS, AssetClass::Crypto),
            365.0
        );
    }

    #[test]
    fn weekly_spacing_uses_calendar_weeks() {
        let periods = infer_periods_per_year_from_spacing(7.0 * DAY_SECS, AssetClass::Equity);
        assert!((periods - 365.0 / 7.0).abs() < 1e-12);
    }
}
