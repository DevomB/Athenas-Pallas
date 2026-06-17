//! Provider-native bar interval labels and validation helpers.
//!
//! Alpha Vantage's non-premium fetch path here downloads daily bars. Intraday Alpha
//! Vantage is a separate premium endpoint and is intentionally not wired into `pallas-fetch`.

/// Alpha Vantage intervals supported by this fetcher.
pub const ALPHA_VANTAGE_INTERVALS: &[&str] = &["1d", "daily"];

/// Provider name for fetch CLI/integration callers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchProvider {
    AlphaVantage,
}

impl FetchProvider {
    /// All documented intervals for this provider.
    pub fn documented_intervals(self) -> &'static [&'static str] {
        match self {
            FetchProvider::AlphaVantage => ALPHA_VANTAGE_INTERVALS,
        }
    }

    /// Parse provider string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "alpha-vantage" | "alphavantage" | "alpha" | "av" => Some(FetchProvider::AlphaVantage),
            _ => None,
        }
    }
}

/// True if `interval` is in the provider's documented list.
pub fn is_documented_interval(provider: FetchProvider, interval: &str) -> bool {
    provider.documented_intervals().contains(&interval)
}

/// Normalize user input.
pub fn normalize_interval(interval: &str) -> String {
    interval.trim().to_ascii_lowercase()
}

/// Human-readable hint when interval is not supported by this fetcher.
pub fn interval_hint(provider: FetchProvider, interval: &str) -> Option<String> {
    if is_documented_interval(provider, interval) {
        return None;
    }
    Some(format!(
        "interval '{interval}' is not supported by {}; this fetcher downloads daily bars",
        match provider {
            FetchProvider::AlphaVantage => "Alpha Vantage",
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_vantage_includes_daily() {
        assert!(is_documented_interval(FetchProvider::AlphaVantage, "1d"));
        assert!(is_documented_interval(FetchProvider::AlphaVantage, "daily"));
    }

    #[test]
    fn intraday_interval_warns() {
        assert!(!is_documented_interval(FetchProvider::AlphaVantage, "5min"));
        assert!(interval_hint(FetchProvider::AlphaVantage, "5min").is_some());
    }
}
