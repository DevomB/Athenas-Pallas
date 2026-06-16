//! Binance Spot klines (`GET /api/v3/klines`).

use super::{write_ohlcv_csv, OhlcvBar};
use crate::error::{Error, Result};
use rust_decimal::Decimal;
use std::path::Path;
use time::OffsetDateTime;

const BASE_URL: &str = "https://api.binance.com";

/// Fetch klines and write OHLCV CSV.
pub async fn fetch_klines_csv(
    client: &reqwest::Client,
    symbol: &str,
    interval: &str,
    start_ms: i64,
    end_ms: i64,
    output: &Path,
) -> Result<()> {
    let bars = fetch_klines(client, symbol, interval, start_ms, end_ms).await?;
    write_ohlcv_csv(output, &bars)
}

/// Paginated kline download.
pub async fn fetch_klines(
    client: &reqwest::Client,
    symbol: &str,
    interval: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<OhlcvBar>> {
    let mut all = Vec::new();
    let mut cursor = start_ms;
    while cursor < end_ms {
        let url = format!(
            "{BASE_URL}/api/v3/klines?symbol={symbol}&interval={interval}&startTime={cursor}&endTime={end_ms}&limit=1000"
        );
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::Invalid(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(Error::Invalid(format!("binance klines: {}", resp.status())));
        }
        let chunk: Vec<serde_json::Value> = resp
            .json()
            .await
            .map_err(|e| Error::Invalid(e.to_string()))?;
        if chunk.is_empty() {
            break;
        }
        for row in &chunk {
            all.push(parse_kline_row(row)?);
        }
        let last_open = chunk
            .last()
            .and_then(|r| r.get(0))
            .and_then(|v| v.as_i64())
            .unwrap_or(end_ms);
        cursor = last_open + 1;
        if chunk.len() < 1000 {
            break;
        }
    }
    Ok(all)
}

fn parse_kline_row(row: &serde_json::Value) -> Result<OhlcvBar> {
    let arr = row
        .as_array()
        .ok_or_else(|| Error::Invalid("kline not array".into()))?;
    let open_ms = arr[0]
        .as_i64()
        .ok_or_else(|| Error::Invalid("open time".into()))?;
    let ts = OffsetDateTime::from_unix_timestamp(open_ms / 1000)
        .map_err(|_| Error::Invalid("timestamp".into()))?;
    Ok(OhlcvBar {
        ts,
        open: parse_decimal_field(&arr[1])?,
        high: parse_decimal_field(&arr[2])?,
        low: parse_decimal_field(&arr[3])?,
        close: parse_decimal_field(&arr[4])?,
        volume: parse_decimal_field(&arr[5])?,
    })
}

fn parse_decimal_field(v: &serde_json::Value) -> Result<Decimal> {
    let s = v
        .as_str()
        .ok_or_else(|| Error::Invalid("decimal field".into()))?;
    s.parse()
        .map_err(|_| Error::Invalid(format!("bad decimal: {s}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn parses_klines_fixture() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/klines"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                [
                    1704067200000i64,
                    "42000.0",
                    "42500.0",
                    "41800.0",
                    "42200.0",
                    "100.5",
                    1704153599999i64,
                    "0",
                    10,
                    "50",
                    "0",
                    "0"
                ],
                [
                    1704153600000i64,
                    "42200.0",
                    "43000.0",
                    "42100.0",
                    "42800.0",
                    "120.0",
                    1704239999999i64,
                    "0",
                    12,
                    "60",
                    "0",
                    "0"
                ],
                [
                    1704240000000i64,
                    "42800.0",
                    "43200.0",
                    "42700.0",
                    "43100.0",
                    "90.0",
                    1704326399999i64,
                    "0",
                    8,
                    "40",
                    "0",
                    "0"
                ]
            ])))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let url_base = server.uri();
        let symbol = "BTCUSDT";
        let interval = "1d";
        let start = 1704067200000i64;
        let end = 1705000000000i64;
        let url = format!(
            "{url_base}/api/v3/klines?symbol={symbol}&interval={interval}&startTime={start}&endTime={end}&limit=1000"
        );
        let resp = client.get(&url).send().await.unwrap();
        let chunk: Vec<serde_json::Value> = resp.json().await.unwrap();
        assert_eq!(chunk.len(), 3);
        let bar = parse_kline_row(&chunk[0]).unwrap();
        assert_eq!(bar.close, Decimal::new(42200, 0));
    }
}
