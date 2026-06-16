//! Yahoo Finance chart API (v8).

use super::{write_yahoo_csv, YahooBar};
use crate::error::{Error, Result};
use rust_decimal::Decimal;
use std::path::Path;

const CHART_URL: &str = "https://query1.finance.yahoo.com/v8/finance/chart";

/// Fetch daily bars and write Yahoo-format CSV.
pub async fn fetch_chart_csv(
    client: &reqwest::Client,
    symbol: &str,
    interval: &str,
    range: &str,
    output: &Path,
) -> Result<()> {
    let bars = fetch_chart(client, symbol, interval, range).await?;
    write_yahoo_csv(output, &bars)
}

/// Download chart JSON and parse OHLCV arrays.
pub async fn fetch_chart(
    client: &reqwest::Client,
    symbol: &str,
    interval: &str,
    range: &str,
) -> Result<Vec<YahooBar>> {
    let url = format!("{CHART_URL}/{symbol}?interval={interval}&range={range}");
    let resp = client
        .get(&url)
        .header("User-Agent", "pallas-fetch/1.0")
        .send()
        .await
        .map_err(|e| Error::Invalid(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(Error::Invalid(format!("yahoo chart: {}", resp.status())));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| Error::Invalid(e.to_string()))?;
    parse_chart_json(&body)
}

pub fn parse_chart_json(body: &serde_json::Value) -> Result<Vec<YahooBar>> {
    let result = body
        .pointer("/chart/result/0")
        .ok_or_else(|| Error::Invalid("yahoo: missing result".into()))?;
    let timestamps = result
        .pointer("/timestamp")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Invalid("yahoo: missing timestamp".into()))?;
    let quote = result
        .pointer("/indicators/quote/0")
        .ok_or_else(|| Error::Invalid("yahoo: missing quote".into()))?;
    let empty: &[serde_json::Value] = &[];
    let opens = quote
        .get("open")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(empty);
    let highs = quote
        .get("high")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(empty);
    let lows = quote
        .get("low")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(empty);
    let closes = quote
        .get("close")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(empty);
    let volumes = quote
        .get("volume")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(empty);
    let adjcloses = result
        .pointer("/indicators/adjclose/0/adjclose")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(empty);

    let mut bars = Vec::with_capacity(timestamps.len());
    for (i, ts) in timestamps.iter().enumerate() {
        let Some(sec) = ts.as_i64() else { continue };
        let close = json_f64(closes.get(i))?;
        if close.is_none() {
            continue;
        }
        let date = time::OffsetDateTime::from_unix_timestamp(sec)
            .map(|d| d.date().to_string())
            .unwrap_or_else(|_| sec.to_string());
        bars.push(YahooBar {
            date,
            open: json_f64(opens.get(i))?.unwrap_or(Decimal::ZERO),
            high: json_f64(highs.get(i))?.unwrap_or(Decimal::ZERO),
            low: json_f64(lows.get(i))?.unwrap_or(Decimal::ZERO),
            close: close.unwrap(),
            adj_close: json_f64(adjcloses.get(i))?,
            volume: json_f64(volumes.get(i))?.unwrap_or(Decimal::ZERO),
        });
    }
    Ok(bars)
}

fn json_f64(v: Option<&serde_json::Value>) -> Result<Option<Decimal>> {
    let Some(v) = v else {
        return Ok(None);
    };
    if v.is_null() {
        return Ok(None);
    }
    let n = v
        .as_f64()
        .ok_or_else(|| Error::Invalid("yahoo number".into()))?;
    Ok(rust_decimal::Decimal::from_f64_retain(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture() {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/yahoo_chart.json")).unwrap();
        let bars = parse_chart_json(&json).unwrap();
        assert_eq!(bars.len(), 5);
        assert!(bars[0].close > Decimal::ZERO);
    }
}
