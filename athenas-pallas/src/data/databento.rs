//! Databento historical data helpers.
//!
//! This module fetches Databento OHLCV schemas and writes the existing engine
//! CSV layout: `ts,open,high,low,close,volume`.

use std::error::Error;
use std::path::Path;

use databento::{
    dbn::{record::OhlcvMsg, Record, SType, Schema, UNDEF_PRICE},
    historical::timeseries::GetRangeParams,
    HistoricalClient,
};
use rust_decimal::Decimal;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Boxed error used by the Databento adapter.
pub type DatabentoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Parameters for a historical Databento OHLCV request.
#[derive(Clone, Debug)]
pub struct DatabentoOhlcvRequest {
    /// Dataset code, e.g. `GLBX.MDP3`, `XNAS.ITCH`, or `EQUS.MINI`.
    pub dataset: String,
    /// Databento symbol expression.
    pub symbol: String,
    /// Input symbology. Defaults to `raw_symbol` in the CLI.
    pub stype_in: SType,
    /// OHLCV schema: `ohlcv-1s`, `ohlcv-1m`, `ohlcv-1h`, or `ohlcv-1d`.
    pub schema: Schema,
    /// Inclusive UTC start.
    pub start: OffsetDateTime,
    /// Exclusive UTC end.
    pub end: OffsetDateTime,
}

impl DatabentoOhlcvRequest {
    /// Build Databento client params.
    pub fn get_range_params(&self) -> GetRangeParams {
        GetRangeParams::builder()
            .dataset(&self.dataset)
            .symbols(self.symbol.as_str())
            .stype_in(self.stype_in)
            .schema(self.schema)
            .date_time_range(self.start..self.end)
            .build()
    }
}

/// Fetch OHLCV bars from Databento and write engine-compatible CSV.
///
/// The API key is read by the official client from `DATABENTO_API_KEY`.
pub async fn fetch_ohlcv_csv(req: &DatabentoOhlcvRequest, output: &Path) -> DatabentoResult<u64> {
    if !matches!(
        req.schema,
        Schema::Ohlcv1S | Schema::Ohlcv1M | Schema::Ohlcv1H | Schema::Ohlcv1D
    ) {
        return Err(format!(
            "schema {} is not an intraday/daily OHLCV schema",
            req.schema
        )
        .into());
    }
    if req.start >= req.end {
        return Err("start must be before end".into());
    }

    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }

    let mut client = HistoricalClient::builder().key_from_env()?.build()?;
    let params = req.get_range_params();
    let mut decoder = client.timeseries().get_range(&params).await?;
    let mut writer = csv::Writer::from_path(output)?;
    writer.write_record(["ts", "open", "high", "low", "close", "volume"])?;

    let mut written = 0u64;
    while let Some(bar) = decoder.decode_record::<OhlcvMsg>().await? {
        let row = ohlcv_row(bar)?;
        writer.write_record(row)?;
        written += 1;
    }
    writer.flush()?;
    Ok(written)
}

fn ohlcv_row(bar: &OhlcvMsg) -> DatabentoResult<[String; 6]> {
    let ts = bar
        .header()
        .ts_event()
        .ok_or("Databento OHLCV record has undefined ts_event")?
        .format(&Rfc3339)?;
    Ok([
        ts,
        price_to_decimal(bar.open)?.to_string(),
        price_to_decimal(bar.high)?.to_string(),
        price_to_decimal(bar.low)?.to_string(),
        price_to_decimal(bar.close)?.to_string(),
        Decimal::from(bar.volume).to_string(),
    ])
}

fn price_to_decimal(raw: i64) -> DatabentoResult<Decimal> {
    if raw == UNDEF_PRICE {
        return Err("Databento OHLCV record contains UNDEF_PRICE".into());
    }
    Ok(Decimal::new(raw, 9).normalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use databento::dbn::{rtype, RecordHeader};

    #[test]
    fn converts_fixed_point_ohlcv_to_engine_csv_row() {
        let bar = OhlcvMsg {
            hd: RecordHeader::new::<OhlcvMsg>(rtype::OHLCV_1M, 1, 2, 1_700_000_000_000_000_000),
            open: 100_000_000_000,
            high: 101_250_000_000,
            low: 99_500_000_000,
            close: 100_750_000_000,
            volume: 42,
        };
        let row = ohlcv_row(&bar).unwrap();
        assert_eq!(row[0], "2023-11-14T22:13:20Z");
        assert_eq!(row[1], "100");
        assert_eq!(row[2], "101.25");
        assert_eq!(row[3], "99.5");
        assert_eq!(row[4], "100.75");
        assert_eq!(row[5], "42");
    }
}
