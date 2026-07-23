//! Databento historical OHLCV fetch and CSV cache support.

use std::fs::{self, File};
use std::io::{self, BufWriter, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use databento::{
    dbn::{OhlcvMsg, SType, Schema},
    historical::{
        metadata::{DatasetRange, GetCostParams},
        timeseries::GetRangeParams,
    },
    HistoricalClient,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{Date, OffsetDateTime, PrimitiveDateTime};

use crate::error::{Error, Result};

const PRICE_SCALE: i128 = 1_000_000_000;

/// Historical Databento OHLCV schemas accepted by the engine CSV cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabentoOhlcvSchema {
    /// One-second bars.
    Ohlcv1S,
    /// One-minute bars.
    Ohlcv1M,
    /// One-hour bars.
    Ohlcv1H,
    /// One-day bars.
    Ohlcv1D,
}

impl DatabentoOhlcvSchema {
    /// Parse a CLI schema string.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ohlcv-1s" => Ok(Self::Ohlcv1S),
            "ohlcv-1m" => Ok(Self::Ohlcv1M),
            "ohlcv-1h" => Ok(Self::Ohlcv1H),
            "ohlcv-1d" => Ok(Self::Ohlcv1D),
            other => Err(Error::Invalid(format!(
                "unsupported databento schema '{other}'; supported OHLCV schemas are ohlcv-1s, ohlcv-1m, ohlcv-1h, ohlcv-1d"
            ))),
        }
    }

    /// Databento DBN schema value.
    pub fn as_dbn(self) -> Schema {
        match self {
            Self::Ohlcv1S => Schema::Ohlcv1S,
            Self::Ohlcv1M => Schema::Ohlcv1M,
            Self::Ohlcv1H => Schema::Ohlcv1H,
            Self::Ohlcv1D => Schema::Ohlcv1D,
        }
    }

    /// CLI/cache string value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ohlcv1S => "ohlcv-1s",
            Self::Ohlcv1M => "ohlcv-1m",
            Self::Ohlcv1H => "ohlcv-1h",
            Self::Ohlcv1D => "ohlcv-1d",
        }
    }
}

/// Databento input symbology type accepted at the CLI boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabentoSType {
    /// Publisher/raw symbol.
    RawSymbol,
    /// Numeric Databento instrument id.
    InstrumentId,
    /// Continuous symbology.
    Continuous,
    /// Parent symbology.
    Parent,
    /// Nasdaq equity suffix symbology.
    NasdaqSymbol,
    /// CMS equity suffix symbology.
    CmsSymbol,
}

impl DatabentoSType {
    /// Parse a CLI stype string.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "raw_symbol" => Ok(Self::RawSymbol),
            "instrument_id" => Ok(Self::InstrumentId),
            "continuous" => Ok(Self::Continuous),
            "parent" => Ok(Self::Parent),
            "nasdaq_symbol" => Ok(Self::NasdaqSymbol),
            "cms_symbol" => Ok(Self::CmsSymbol),
            other => Err(Error::Invalid(format!(
                "unsupported databento stype_in '{other}'; supported values are raw_symbol, instrument_id, continuous, parent, nasdaq_symbol, cms_symbol"
            ))),
        }
    }

    /// Databento DBN symbology value.
    pub fn as_dbn(self) -> SType {
        match self {
            Self::RawSymbol => SType::RawSymbol,
            Self::InstrumentId => SType::InstrumentId,
            Self::Continuous => SType::Continuous,
            Self::Parent => SType::Parent,
            Self::NasdaqSymbol => SType::NasdaqSymbol,
            Self::CmsSymbol => SType::CmsSymbol,
        }
    }

    /// CLI/cache string value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RawSymbol => "raw_symbol",
            Self::InstrumentId => "instrument_id",
            Self::Continuous => "continuous",
            Self::Parent => "parent",
            Self::NasdaqSymbol => "nasdaq_symbol",
            Self::CmsSymbol => "cms_symbol",
        }
    }
}

/// Typed historical Databento request configuration.
#[derive(Clone, Debug)]
pub struct DatabentoFetchConfig {
    /// Databento dataset code.
    pub dataset: String,
    /// Requested symbol.
    pub symbol: String,
    /// OHLCV aggregation schema.
    pub schema: DatabentoOhlcvSchema,
    /// Inclusive UTC start.
    pub start: OffsetDateTime,
    /// Exclusive UTC end.
    pub end: OffsetDateTime,
    /// Input symbol type.
    pub stype_in: DatabentoSType,
    /// Cache directory.
    pub cache_dir: PathBuf,
    /// Replace existing cache.
    pub refresh_data: bool,
    /// Cost warning threshold in USD.
    pub cost_warning_usd: f64,
    /// Continue without prompting when above the cost warning.
    pub yes: bool,
    /// Estimate cost and exit without fetching.
    pub estimate_only: bool,
}

/// Result of resolving a Databento cache request.
#[derive(Clone, Debug)]
pub struct DatabentoCacheResult {
    /// Final engine CSV path.
    pub cache_path: PathBuf,
    /// Estimated Databento cost in USD when an API estimate was needed.
    pub estimated_cost_usd: Option<f64>,
    /// Whether data was fetched during this call.
    pub fetched: bool,
}

/// Versioned provenance for a materialized raw Databento CSV.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DatabentoCacheManifest {
    /// Manifest schema version.
    pub version: u32,
    /// Databento dataset code.
    pub dataset: String,
    /// Requested publisher/raw input symbols.
    pub input_symbols: Vec<String>,
    /// Input symbology.
    pub stype_in: String,
    /// Databento schema.
    pub schema: String,
    /// Inclusive request start.
    pub start: String,
    /// Exclusive request end.
    pub end: String,
    /// Retrieval timestamp.
    pub retrieved_at: String,
    /// Materialized row count.
    pub source_row_count: usize,
    /// SHA-256 of the final raw CSV.
    pub raw_sha256: String,
    /// Explicit adjustment policy.
    pub adjustment_mode: String,
    /// Databento Rust client compatibility line.
    pub databento_client: String,
}

/// Read-only request capability and cost inspection.
#[derive(Clone, Debug, Serialize)]
pub struct DatabentoInspection {
    /// Requested dataset.
    pub dataset: String,
    /// Requested OHLCV schema.
    pub requested_schema: String,
    /// Schemas currently advertised for the dataset.
    pub available_schemas: Vec<String>,
    /// Entitled dataset start in UTC.
    pub dataset_start: String,
    /// Entitled dataset end in UTC.
    pub dataset_end: String,
    /// Requested schema start in UTC.
    pub schema_start: String,
    /// Requested schema end in UTC.
    pub schema_end: String,
    /// Whether point-in-time definition records are advertised.
    pub definitions_available: bool,
    /// Exact request cost estimate in USD.
    pub estimated_cost_usd: f64,
    /// Cache path a paid fetch would use.
    pub planned_cache_path: String,
}

/// Parse a Databento CLI datetime.
///
/// Date-only values must use American `MM-DD-YYYY` format and are interpreted as UTC midnight.
pub fn parse_datetime(value: &str) -> Result<OffsetDateTime> {
    let value = value.trim();
    if let Ok(dt) = OffsetDateTime::parse(value, &Rfc3339) {
        return Ok(dt);
    }
    let date_fmt = format_description!("[month]-[day]-[year]");
    if let Ok(date) = Date::parse(value, &date_fmt) {
        return Ok(date
            .with_hms(0, 0, 0)
            .map_err(|err| Error::Invalid(format!("invalid databento date '{value}': {err}")))?
            .assume_utc());
    }
    let datetime_fmt = format_description!("[month]-[day]-[year] [hour]:[minute]:[second]");
    if let Ok(dt) = PrimitiveDateTime::parse(value, &datetime_fmt) {
        return Ok(dt.assume_utc());
    }
    Err(Error::Invalid(format!(
        "invalid databento datetime '{value}'; use American MM-DD-YYYY format, e.g. 01-31-2025, or RFC3339"
    )))
}

/// Compute the engine CSV cache path for a Databento request.
pub fn cache_path(cfg: &DatabentoFetchConfig) -> PathBuf {
    cfg.cache_dir.join(format!(
        "{}_{}_{}_{}_{}.csv",
        sanitize_segment(&cfg.dataset),
        sanitize_segment(&cfg.symbol),
        cfg.schema.as_str(),
        cache_datetime(cfg.start),
        cache_datetime(cfg.end)
    ))
}

/// JSON output path for a read-only request inspection.
pub fn inspection_path(cfg: &DatabentoFetchConfig) -> PathBuf {
    cache_path(cfg).with_extension("inspect.json")
}

/// JSON provenance path paired with the raw CSV cache.
pub fn manifest_path(cfg: &DatabentoFetchConfig) -> PathBuf {
    cache_path(cfg).with_extension("manifest.json")
}

/// Resolve or fetch the cached Databento CSV.
pub fn ensure_cached_csv(cfg: &DatabentoFetchConfig) -> Result<DatabentoCacheResult> {
    if cfg.end <= cfg.start {
        return Err(Error::Invalid(
            "invalid databento range: --end must be after --start".to_string(),
        ));
    }
    if cfg.cost_warning_usd < 0.0 {
        return Err(Error::Invalid(
            "invalid databento cost warning: --cost-warning-usd must be >= 0".to_string(),
        ));
    }

    let path = cache_path(cfg);
    if !cfg.refresh_data && !cfg.estimate_only && cached_request_is_valid(cfg, &path)? {
        return Ok(DatabentoCacheResult {
            cache_path: path,
            estimated_cost_usd: None,
            fetched: false,
        });
    }

    dotenvy::dotenv().ok();
    require_api_key()?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| Error::Invalid(format!("databento async runtime failure: {err}")))?;
    let inspection = rt.block_on(inspect_request(cfg))?;
    let estimated_cost_usd = inspection.estimated_cost_usd;
    println!(
        "Databento estimated cost: ${estimated_cost_usd:.6}; cache: {}",
        path.display()
    );

    if cfg.estimate_only {
        write_json_atomic(&inspection_path(cfg), &inspection)?;
        return Ok(DatabentoCacheResult {
            cache_path: path,
            estimated_cost_usd: Some(estimated_cost_usd),
            fetched: false,
        });
    }

    confirm_cost(cfg, estimated_cost_usd)?;
    let source_row_count = rt.block_on(fetch_to_cache(cfg, &path))?;
    let manifest = cache_manifest(cfg, &path, source_row_count)?;
    write_json_atomic(&manifest_path(cfg), &manifest)?;

    Ok(DatabentoCacheResult {
        cache_path: path,
        estimated_cost_usd: Some(estimated_cost_usd),
        fetched: true,
    })
}

async fn inspect_request(cfg: &DatabentoFetchConfig) -> Result<DatabentoInspection> {
    let mut client = client_from_env()?;
    let mut available_schemas = client
        .metadata()
        .list_schemas(&cfg.dataset)
        .await
        .map_err(map_api_error)?;
    available_schemas.sort_by_key(|schema| schema.as_str());
    let range = client
        .metadata()
        .get_dataset_range(&cfg.dataset)
        .await
        .map_err(map_api_error)?;
    let schema_range = validate_inspection(cfg, &available_schemas, &range)?;
    let params = GetCostParams::builder()
        .dataset(&cfg.dataset)
        .symbols(cfg.symbol.as_str())
        .schema(cfg.schema.as_dbn())
        .date_time_range(cfg.start..cfg.end)
        .stype_in(cfg.stype_in.as_dbn())
        .build();
    let estimated_cost_usd = client
        .metadata()
        .get_cost(&params)
        .await
        .map_err(map_api_error)?;
    Ok(DatabentoInspection {
        dataset: cfg.dataset.clone(),
        requested_schema: cfg.schema.as_str().into(),
        available_schemas: available_schemas
            .iter()
            .map(|schema| schema.as_str().to_string())
            .collect(),
        dataset_start: format_utc(range.start)?,
        dataset_end: format_utc(range.end)?,
        schema_start: format_utc(schema_range.start)?,
        schema_end: format_utc(schema_range.end)?,
        definitions_available: available_schemas.contains(&Schema::Definition),
        estimated_cost_usd,
        planned_cache_path: cache_path(cfg).display().to_string(),
    })
}

fn validate_inspection<'a>(
    cfg: &DatabentoFetchConfig,
    schemas: &[Schema],
    range: &'a DatasetRange,
) -> Result<&'a databento::historical::DateTimeRange> {
    let schema = cfg.schema.as_dbn();
    if !schemas.contains(&schema) {
        return Err(Error::Invalid(format!(
            "databento dataset '{}' does not advertise schema '{}'",
            cfg.dataset,
            cfg.schema.as_str()
        )));
    }
    let schema_range = range.range_by_schema.get(&schema).ok_or_else(|| {
        Error::Invalid(format!(
            "databento dataset '{}' did not return an entitled range for schema '{}'",
            cfg.dataset,
            cfg.schema.as_str()
        ))
    })?;
    if cfg.start < schema_range.start || cfg.end > schema_range.end {
        return Err(Error::Invalid(format!(
            "databento request {}..{} falls outside entitled {} range {}..{}",
            cfg.start,
            cfg.end,
            cfg.schema.as_str(),
            schema_range.start,
            schema_range.end
        )));
    }
    Ok(schema_range)
}

fn format_utc(value: OffsetDateTime) -> Result<String> {
    value
        .format(&Rfc3339)
        .map_err(|error| Error::Invalid(format!("invalid databento metadata timestamp: {error}")))
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        Error::Invalid(format!(
            "invalid databento metadata path '{}': missing parent directory",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent)?;
    let tmp = path.with_extension("json.tmp");
    let mut writer = BufWriter::new(File::create(&tmp)?);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.flush()?;
    drop(writer);
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}

async fn fetch_to_cache(cfg: &DatabentoFetchConfig, path: &Path) -> Result<usize> {
    let parent = path.parent().ok_or_else(|| {
        Error::Invalid(format!(
            "invalid databento cache path '{}': missing parent directory",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        Error::Invalid(format!(
            "invalid databento cache path '{}': {err}",
            parent.display()
        ))
    })?;

    let tmp_path = path.with_extension("csv.tmp");
    let mut client = client_from_env()?;
    let params = GetRangeParams::builder()
        .dataset(&cfg.dataset)
        .symbols(cfg.symbol.as_str())
        .schema(cfg.schema.as_dbn())
        .date_time_range(cfg.start..cfg.end)
        .stype_in(cfg.stype_in.as_dbn())
        .build();
    let mut decoder = client
        .timeseries()
        .get_range(&params)
        .await
        .map_err(map_api_error)?;

    let file = File::create(&tmp_path).map_err(|err| {
        Error::Invalid(format!(
            "invalid databento cache path '{}': {err}",
            tmp_path.display()
        ))
    })?;
    let mut writer = BufWriter::new(file);
    writeln!(writer, "ts,open,high,low,close,volume")?;

    let mut rows = 0usize;
    while let Some(bar) = decoder.decode_record::<OhlcvMsg>().await.map_err(|err| {
        Error::Invalid(format!(
            "malformed databento data: failed decoding OHLCV row: {err}"
        ))
    })? {
        write_bar(&mut writer, bar)?;
        rows += 1;
    }
    if rows == 0 {
        let _ = fs::remove_file(&tmp_path);
        return Err(Error::Invalid(
            "malformed databento data: decoded zero OHLCV rows".to_string(),
        ));
    }

    writer.flush()?;
    drop(writer);
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&tmp_path, path).map_err(|err| {
        Error::Invalid(format!(
            "invalid databento cache path '{}': {err}",
            path.display()
        ))
    })?;
    Ok(rows)
}

fn cache_manifest(
    cfg: &DatabentoFetchConfig,
    path: &Path,
    source_row_count: usize,
) -> Result<DatabentoCacheManifest> {
    Ok(DatabentoCacheManifest {
        version: 1,
        dataset: cfg.dataset.clone(),
        input_symbols: vec![cfg.symbol.clone()],
        stype_in: cfg.stype_in.as_str().into(),
        schema: cfg.schema.as_str().into(),
        start: format_utc(cfg.start)?,
        end: format_utc(cfg.end)?,
        retrieved_at: format_utc(OffsetDateTime::now_utc())?,
        source_row_count,
        raw_sha256: sha256_file(path)?,
        adjustment_mode: "raw".into(),
        databento_client: "databento-rs 0.53.x".into(),
    })
}

fn cached_request_is_valid(cfg: &DatabentoFetchConfig, path: &Path) -> Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    let manifest_path = manifest_path(cfg);
    if !manifest_path.is_file() {
        return Ok(false);
    }
    let manifest: DatabentoCacheManifest =
        match serde_json::from_reader(File::open(&manifest_path)?) {
            Ok(manifest) => manifest,
            Err(_) => return Ok(false),
        };
    if manifest.version != 1
        || manifest.dataset != cfg.dataset
        || manifest.input_symbols != [cfg.symbol.as_str()]
        || manifest.stype_in != cfg.stype_in.as_str()
        || manifest.schema != cfg.schema.as_str()
        || manifest.start != format_utc(cfg.start)?
        || manifest.end != format_utc(cfg.end)?
        || manifest.adjustment_mode != "raw"
    {
        return Ok(false);
    }
    Ok(manifest.raw_sha256 == sha256_file(path)?)
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn write_bar(writer: &mut BufWriter<File>, bar: &OhlcvMsg) -> Result<()> {
    let ts = bar.hd.ts_event().ok_or_else(|| {
        Error::Invalid("malformed databento data: OHLCV row missing ts_event".to_string())
    })?;
    let ts = ts
        .format(&Rfc3339)
        .map_err(|err| Error::Invalid(format!("malformed databento timestamp: {err}")))?;
    writeln!(
        writer,
        "{},{},{},{},{},{}",
        ts,
        format_fixed_price(bar.open),
        format_fixed_price(bar.high),
        format_fixed_price(bar.low),
        format_fixed_price(bar.close),
        bar.volume
    )?;
    Ok(())
}

fn client_from_env() -> Result<HistoricalClient> {
    HistoricalClient::builder()
        .key_from_env()
        .map_err(|err| Error::Invalid(format!("databento missing API key: {err}")))?
        .build()
        .map_err(map_api_error)
}

fn require_api_key() -> Result<()> {
    match std::env::var("DATABENTO_API_KEY") {
        Ok(value) if !value.trim().is_empty() => Ok(()),
        _ => Err(Error::Invalid(
            "databento missing API key: set DATABENTO_API_KEY in the repo-root .env before fetching or estimating uncached data".to_string(),
        )),
    }
}

fn confirm_cost(cfg: &DatabentoFetchConfig, estimated_cost_usd: f64) -> Result<()> {
    if estimated_cost_usd <= cfg.cost_warning_usd {
        return Ok(());
    }
    if cfg.yes {
        eprintln!(
            "Databento cost warning: estimated ${estimated_cost_usd:.6} exceeds threshold ${:.6}; continuing because --yes was supplied.",
            cfg.cost_warning_usd
        );
        return Ok(());
    }
    eprintln!(
        "Databento cost warning: estimated ${estimated_cost_usd:.6} exceeds threshold ${:.6}.",
        cfg.cost_warning_usd
    );
    if !io::stdin().is_terminal() {
        return Err(Error::Invalid(
            "databento cost warning: non-interactive run aborted; rerun with --yes to continue"
                .to_string(),
        ));
    }
    eprint!("Continue with fetch? Type yes to continue: ");
    io::stderr().flush()?;
    let mut response = String::new();
    io::stdin().read_line(&mut response)?;
    match response.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Ok(()),
        _ => Err(Error::Invalid(
            "databento cost warning: fetch aborted by user".to_string(),
        )),
    }
}

fn map_api_error(err: databento::Error) -> Error {
    Error::Invalid(format!("databento API/entitlement failure: {err}"))
}

fn format_fixed_price(value: i64) -> String {
    let value = i128::from(value);
    let negative = value < 0;
    let abs = if negative { -value } else { value };
    let whole = abs / PRICE_SCALE;
    let frac = abs % PRICE_SCALE;
    let sign = if negative { "-" } else { "" };
    if frac == 0 {
        return format!("{sign}{whole}");
    }
    let mut frac_text = format!("{frac:09}");
    while frac_text.ends_with('0') {
        frac_text.pop();
    }
    format!("{sign}{whole}.{frac_text}")
}

fn sanitize_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

fn cache_datetime(value: OffsetDateTime) -> String {
    value
        .format(&format_description!(
            "[year][month][day]T[hour][minute][second]Z"
        ))
        .expect("cache datetime format is static and valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> DatabentoFetchConfig {
        DatabentoFetchConfig {
            dataset: "EQUS.MINI".to_string(),
            symbol: "AAPL".to_string(),
            schema: DatabentoOhlcvSchema::Ohlcv1D,
            start: parse_datetime("01-01-2025").unwrap(),
            end: parse_datetime("02-01-2025").unwrap(),
            stype_in: DatabentoSType::RawSymbol,
            cache_dir: PathBuf::from("data/databento"),
            refresh_data: false,
            cost_warning_usd: 1.0,
            yes: false,
            estimate_only: false,
        }
    }

    #[test]
    fn parses_only_ohlcv_schemas() {
        assert_eq!(
            DatabentoOhlcvSchema::parse("ohlcv-1d").unwrap(),
            DatabentoOhlcvSchema::Ohlcv1D
        );
        assert!(DatabentoOhlcvSchema::parse("trades").is_err());
    }

    #[test]
    fn parses_supported_stype_in_values() {
        assert_eq!(
            DatabentoSType::parse("raw_symbol").unwrap(),
            DatabentoSType::RawSymbol
        );
        assert!(DatabentoSType::parse("bad").is_err());
    }

    #[test]
    fn formats_fixed_prices_without_float_rounding() {
        assert_eq!(format_fixed_price(123_450_000_000), "123.45");
        assert_eq!(format_fixed_price(1), "0.000000001");
        assert_eq!(format_fixed_price(-1_250_000_000), "-1.25");
    }

    #[test]
    fn builds_stable_cache_path() {
        let path = cache_path(&cfg());
        assert_eq!(
            path,
            PathBuf::from(
                "data/databento/EQUS_MINI_AAPL_ohlcv-1d_20250101T000000Z_20250201T000000Z.csv"
            )
        );
    }

    #[test]
    fn parses_american_date_only_as_utc_midnight() {
        let dt = parse_datetime("01-01-2025").unwrap();
        assert_eq!(
            dt,
            Date::from_calendar_date(2025, time::Month::January, 1)
                .unwrap()
                .midnight()
                .assume_utc()
        );
    }

    #[test]
    fn rejects_iso_date_only_format() {
        let err = parse_datetime("2025-01-01").unwrap_err();
        assert!(err.to_string().contains("American MM-DD-YYYY format"));
    }

    #[test]
    fn inspection_rejects_unavailable_schema_before_cost_or_download() {
        let config = cfg();
        let range = DatasetRange {
            start: parse_datetime("01-01-2020").unwrap(),
            end: parse_datetime("01-01-2026").unwrap(),
            range_by_schema: std::collections::HashMap::new(),
        };
        let error = validate_inspection(&config, &[Schema::Trades], &range).unwrap_err();
        assert!(error.to_string().contains("does not advertise schema"));
    }

    #[test]
    fn inspection_rejects_out_of_range_request() {
        let config = cfg();
        let schema = config.schema.as_dbn();
        let range = DatasetRange {
            start: parse_datetime("01-01-2025").unwrap(),
            end: parse_datetime("01-15-2025").unwrap(),
            range_by_schema: std::collections::HashMap::from([(
                schema,
                databento::historical::DateTimeRange {
                    start: parse_datetime("01-01-2025").unwrap(),
                    end: parse_datetime("01-15-2025").unwrap(),
                },
            )]),
        };
        let error = validate_inspection(&config, &[schema], &range).unwrap_err();
        assert!(error.to_string().contains("outside entitled"));
    }

    #[test]
    fn cache_reuse_requires_matching_manifest_and_checksum() {
        let mut config = cfg();
        let root = std::env::temp_dir().join(format!("pallas-databento-{}", uuid::Uuid::new_v4()));
        config.cache_dir = root.clone();
        let csv = cache_path(&config);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&csv, "ts,open,high,low,close,volume\n").unwrap();
        assert!(!cached_request_is_valid(&config, &csv).unwrap());

        let manifest = cache_manifest(&config, &csv, 0).unwrap();
        write_json_atomic(&manifest_path(&config), &manifest).unwrap();
        assert!(cached_request_is_valid(&config, &csv).unwrap());

        std::fs::write(&csv, "tampered").unwrap();
        assert!(!cached_request_is_valid(&config, &csv).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }
}
