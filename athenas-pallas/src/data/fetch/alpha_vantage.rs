//! Alpha Vantage daily market-data fetcher.

use super::{write_ohlcv_csv, OhlcvBar};
use crate::error::{Error, Result};
use rust_decimal::Decimal;
use serde_json::Value;
use std::path::Path;
use time::{Date, OffsetDateTime, Time};

const BASE_URL: &str = "https://www.alphavantage.co/query";
const DAILY_EQUITY_SERIES: &str = "Time Series (Daily)";
const DAILY_CRYPTO_SERIES: &str = "Time Series (Digital Currency Daily)";

/// Alpha Vantage daily asset family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaVantageAsset {
    /// Equity, ETF, or mutual fund symbol through `TIME_SERIES_DAILY`.
    Equity,
    /// Digital currency through `DIGITAL_CURRENCY_DAILY`.
    Crypto,
}

impl AlphaVantageAsset {
    /// Parse CLI-friendly asset labels.
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "equity" | "stock" | "stocks" | "etf" | "fund" => Some(Self::Equity),
            "crypto" | "cryptocurrency" | "digital-currency" => Some(Self::Crypto),
            _ => None,
        }
    }
}

/// Fetch Alpha Vantage daily bars and write standard Pallas OHLCV CSV.
pub async fn fetch_daily_csv(
    client: &reqwest::Client,
    api_key: &str,
    asset: AlphaVantageAsset,
    symbol: &str,
    market: &str,
    outputsize: &str,
    output: &Path,
) -> Result<()> {
    let bars = fetch_daily(client, api_key, asset, symbol, market, outputsize).await?;
    write_ohlcv_csv(output, &bars)
}

/// Fetch Alpha Vantage daily bars into normalized OHLCV rows.
pub async fn fetch_daily(
    client: &reqwest::Client,
    api_key: &str,
    asset: AlphaVantageAsset,
    symbol: &str,
    market: &str,
    outputsize: &str,
) -> Result<Vec<OhlcvBar>> {
    if api_key.trim().is_empty() {
        return Err(Error::Invalid(
            "ALPHA_VANTAGE_API_KEY is required; set it in the environment or a local .env file"
                .into(),
        ));
    }

    let mut query = vec![
        ("symbol", symbol.to_string()),
        ("apikey", api_key.to_string()),
    ];
    match asset {
        AlphaVantageAsset::Equity => {
            query.push(("function", "TIME_SERIES_DAILY".to_string()));
            query.push(("outputsize", outputsize.to_string()));
        }
        AlphaVantageAsset::Crypto => {
            query.push(("function", "DIGITAL_CURRENCY_DAILY".to_string()));
            query.push(("market", market.to_string()));
        }
    }

    let resp = client
        .get(BASE_URL)
        .query(&query)
        .send()
        .await
        .map_err(|e| Error::Invalid(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(Error::Invalid(format!(
            "alpha vantage daily fetch: {}",
            resp.status()
        )));
    }

    let body: Value = resp
        .json()
        .await
        .map_err(|e| Error::Invalid(e.to_string()))?;
    parse_daily_json(&body, asset, market)
}

/// Parse Alpha Vantage daily JSON into oldest-first OHLCV rows.
pub fn parse_daily_json(
    body: &Value,
    asset: AlphaVantageAsset,
    market: &str,
) -> Result<Vec<OhlcvBar>> {
    reject_alpha_vantage_error(body)?;
    let series_key = match asset {
        AlphaVantageAsset::Equity => DAILY_EQUITY_SERIES,
        AlphaVantageAsset::Crypto => DAILY_CRYPTO_SERIES,
    };
    let series = body
        .get(series_key)
        .and_then(Value::as_object)
        .ok_or_else(|| Error::Invalid(format!("alpha vantage: missing {series_key}")))?;

    let mut bars = Vec::with_capacity(series.len());
    for (date, row) in series {
        let row = row
            .as_object()
            .ok_or_else(|| Error::Invalid(format!("alpha vantage: bad row for {date}")))?;
        let date = Date::parse(
            date,
            &time::macros::format_description!("[year]-[month]-[day]"),
        )
        .map_err(|e| Error::Invalid(format!("alpha vantage date: {e}")))?;
        let ts = date.with_time(Time::MIDNIGHT).assume_utc();
        bars.push(OhlcvBar {
            ts,
            open: field_decimal(row, &["1. open"], Some(("open", market)))?,
            high: field_decimal(row, &["2. high"], Some(("high", market)))?,
            low: field_decimal(row, &["3. low"], Some(("low", market)))?,
            close: field_decimal(row, &["4. close"], Some(("close", market)))?,
            volume: field_decimal(row, &["5. volume"], Some(("volume", market)))?,
        });
    }

    bars.sort_by_key(|bar| bar.ts);
    Ok(bars)
}

fn reject_alpha_vantage_error(body: &Value) -> Result<()> {
    for key in ["Error Message", "Information", "Note"] {
        if let Some(message) = body.get(key).and_then(Value::as_str) {
            return Err(Error::Invalid(format!("alpha vantage: {message}")));
        }
    }
    Ok(())
}

fn field_decimal(
    row: &serde_json::Map<String, Value>,
    exact_keys: &[&str],
    fallback: Option<(&str, &str)>,
) -> Result<Decimal> {
    for key in exact_keys {
        if let Some(value) = row.get(*key) {
            return decimal_value(value, key);
        }
    }
    if let Some((field, market)) = fallback {
        let market = market.to_ascii_lowercase();
        for (key, value) in row {
            let lower = key.to_ascii_lowercase();
            if lower.contains(field) && (market.is_empty() || lower.contains(&market)) {
                return decimal_value(value, key);
            }
        }
        for (key, value) in row {
            if key.to_ascii_lowercase().contains(field) {
                return decimal_value(value, key);
            }
        }
    }
    Err(Error::Invalid("alpha vantage: missing numeric field".into()))
}

fn decimal_value(value: &Value, key: &str) -> Result<Decimal> {
    if let Some(s) = value.as_str() {
        return s
            .parse()
            .map_err(|_| Error::Invalid(format!("alpha vantage bad decimal: {key}")));
    }
    if let Some(n) = value.as_f64() {
        return Decimal::from_f64_retain(n)
            .ok_or_else(|| Error::Invalid(format!("alpha vantage bad decimal: {key}")));
    }
    Err(Error::Invalid(format!(
        "alpha vantage numeric field is not a string: {key}"
    )))
}

/// Keep only bars in the inclusive `[start, end]` date range.
pub fn filter_date_range(
    bars: Vec<OhlcvBar>,
    start: Option<OffsetDateTime>,
    end: Option<OffsetDateTime>,
) -> Vec<OhlcvBar> {
    bars.into_iter()
        .filter(|bar| start.map_or(true, |s| bar.ts >= s))
        .filter(|bar| end.map_or(true, |e| bar.ts <= e))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_equity_daily_json_oldest_first() {
        let json = serde_json::json!({
            "Time Series (Daily)": {
                "2024-01-03": {
                    "1. open": "102.0",
                    "2. high": "105.0",
                    "3. low": "101.0",
                    "4. close": "104.0",
                    "5. volume": "1200"
                },
                "2024-01-02": {
                    "1. open": "100.0",
                    "2. high": "103.0",
                    "3. low": "99.0",
                    "4. close": "102.0",
                    "5. volume": "1000"
                }
            }
        });
        let bars = parse_daily_json(&json, AlphaVantageAsset::Equity, "USD").unwrap();
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].close, Decimal::new(1020, 1));
        assert_eq!(bars[1].volume, Decimal::new(1200, 0));
    }

    #[test]
    fn parses_crypto_daily_json_market_columns() {
        let json = serde_json::json!({
            "Time Series (Digital Currency Daily)": {
                "2024-01-02": {
                    "1a. open (USD)": "42000.0",
                    "2a. high (USD)": "43000.0",
                    "3a. low (USD)": "41000.0",
                    "4a. close (USD)": "42500.0",
                    "5. volume": "123.45"
                }
            }
        });
        let bars = parse_daily_json(&json, AlphaVantageAsset::Crypto, "USD").unwrap();
        assert_eq!(bars[0].open, Decimal::new(420000, 1));
        assert_eq!(bars[0].volume, Decimal::new(12345, 2));
    }
}
