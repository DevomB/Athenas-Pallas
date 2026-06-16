//! Provider-native bar interval labels and validation helpers.
//!
//! Intervals are passed through to Yahoo/Binance APIs as opaque strings. Lists below
//! document what each provider documents; any other string is still accepted (custom).

/// Binance Spot kline intervals (documented API values).
pub const BINANCE_INTERVALS: &[&str] = &[
    "1s", "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d", "1w",
    "1M",
];

/// Yahoo Finance chart intervals (documented API values).
pub const YAHOO_INTERVALS: &[&str] = &[
    "1m", "2m", "5m", "15m", "30m", "60m", "90m", "1h", "1d", "5d", "1wk", "1mo", "3mo",
];

/// Provider name for fetch CLI / GUI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchProvider {
    Binance,
    Yahoo,
}

impl FetchProvider {
    /// All documented intervals for this provider.
    pub fn documented_intervals(self) -> &'static [&'static str] {
        match self {
            FetchProvider::Binance => BINANCE_INTERVALS,
            FetchProvider::Yahoo => YAHOO_INTERVALS,
        }
    }

    /// Parse CLI/GUI provider string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "binance" => Some(FetchProvider::Binance),
            "yahoo" => Some(FetchProvider::Yahoo),
            _ => None,
        }
    }
}

/// True if `interval` is in the provider's documented list (case-sensitive for Binance `1M`).
pub fn is_documented_interval(provider: FetchProvider, interval: &str) -> bool {
    provider.documented_intervals().contains(&interval)
}

/// Normalize user input: trim whitespace; do not rewrite casing (Binance `1M` vs `1m` matters).
pub fn normalize_interval(interval: &str) -> String {
    interval.trim().to_string()
}

/// Human-readable hint when interval is not in the documented list (still allowed).
pub fn interval_hint(provider: FetchProvider, interval: &str) -> Option<String> {
    if is_documented_interval(provider, interval) {
        return None;
    }
    Some(format!(
        "interval '{interval}' is not in the documented {} list; passing through to API anyway",
        match provider {
            FetchProvider::Binance => "Binance",
            FetchProvider::Yahoo => "Yahoo",
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binance_includes_30m_and_1s() {
        assert!(is_documented_interval(FetchProvider::Binance, "30m"));
        assert!(is_documented_interval(FetchProvider::Binance, "1s"));
    }

    #[test]
    fn yahoo_includes_60m_and_90m() {
        assert!(is_documented_interval(FetchProvider::Yahoo, "60m"));
        assert!(is_documented_interval(FetchProvider::Yahoo, "90m"));
    }

    #[test]
    fn custom_interval_allowed() {
        assert!(!is_documented_interval(FetchProvider::Binance, "7m"));
        assert!(interval_hint(FetchProvider::Binance, "7m").is_some());
    }
}
