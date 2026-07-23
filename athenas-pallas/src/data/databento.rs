//! Databento historical OHLCV fetch and CSV cache support.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use databento::{
    dbn::{OhlcvMsg, SType, Schema},
    historical::{
        metadata::{DatasetRange, GetCostParams},
        timeseries::GetRangeParams,
    },
    HistoricalClient, ReferenceClient,
};
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{Date, OffsetDateTime, PrimitiveDateTime};

use crate::error::{Error, Result};

const PRICE_SCALE: i128 = 1_000_000_000;

/// Explicit Databento OHLCV adjustment policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AdjustmentMode {
    /// Preserve vendor OHLCV without modification.
    #[default]
    Raw,
    /// Back-adjust prices and volume for subdivisions and consolidations only.
    SplitAdjusted,
    /// Back-adjust prices for all active factors and volume for splits only.
    TotalReturnAdjusted,
}

impl AdjustmentMode {
    /// Parse a CLI adjustment policy.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "raw" => Ok(Self::Raw),
            "split-adjusted" => Ok(Self::SplitAdjusted),
            "total-return-adjusted" => Ok(Self::TotalReturnAdjusted),
            other => Err(Error::Invalid(format!(
                "unsupported adjustment mode '{other}'; use raw, split-adjusted, or total-return-adjusted"
            ))),
        }
    }

    /// Stable manifest/CLI name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::SplitAdjusted => "split-adjusted",
            Self::TotalReturnAdjusted => "total-return-adjusted",
        }
    }

    fn accepts_reason(self, reason: u32) -> bool {
        match self {
            Self::Raw => false,
            Self::SplitAdjusted => matches!(reason, 5 | 6),
            Self::TotalReturnAdjusted => true,
        }
    }
}

/// Historical Databento schemas accepted by the engine cache.
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
    /// Tick-by-tick trades.
    Trades,
    /// Top-of-book updates.
    Mbp1,
    /// Ten-level depth snapshots.
    Mbp10,
    /// Venue trading status.
    Status,
    /// Auction imbalance updates.
    Imbalance,
}

impl DatabentoOhlcvSchema {
    /// Parse a CLI schema string.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ohlcv-1s" => Ok(Self::Ohlcv1S),
            "ohlcv-1m" => Ok(Self::Ohlcv1M),
            "ohlcv-1h" => Ok(Self::Ohlcv1H),
            "ohlcv-1d" => Ok(Self::Ohlcv1D),
            "trades" => Ok(Self::Trades),
            "mbp-1" => Ok(Self::Mbp1),
            "mbp-10" => Ok(Self::Mbp10),
            "status" => Ok(Self::Status),
            "imbalance" => Ok(Self::Imbalance),
            other => Err(Error::Invalid(format!(
                "unsupported databento schema '{other}'; use ohlcv-1s/1m/1h/1d, trades, mbp-1, mbp-10, status, or imbalance"
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
            Self::Trades => Schema::Trades,
            Self::Mbp1 => Schema::Mbp1,
            Self::Mbp10 => Schema::Mbp10,
            Self::Status => Schema::Status,
            Self::Imbalance => Schema::Imbalance,
        }
    }

    /// CLI/cache string value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ohlcv1S => "ohlcv-1s",
            Self::Ohlcv1M => "ohlcv-1m",
            Self::Ohlcv1H => "ohlcv-1h",
            Self::Ohlcv1D => "ohlcv-1d",
            Self::Trades => "trades",
            Self::Mbp1 => "mbp-1",
            Self::Mbp10 => "mbp-10",
            Self::Status => "status",
            Self::Imbalance => "imbalance",
        }
    }

    /// Whether this schema materializes the engine OHLCV CSV contract.
    pub fn is_ohlcv(self) -> bool {
        matches!(
            self,
            Self::Ohlcv1S | Self::Ohlcv1M | Self::Ohlcv1H | Self::Ohlcv1D
        )
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
    /// Raw, split-adjusted, or total-return-adjusted materialization.
    pub adjustment_mode: AdjustmentMode,
    /// Fetch and persist point-in-time instrument definitions.
    pub import_definitions: bool,
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
    /// Persisted typed point-in-time definitions when requested.
    pub definitions_path: Option<PathBuf>,
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

/// One persisted adjustment-factor record, including non-applied statuses.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdjustmentFactorRecord {
    /// Corporate-action event identifier.
    pub event_id: String,
    /// Effective date.
    pub ex_date: String,
    /// `apply`, `pending`, or `rescind`.
    pub status: String,
    /// Vendor factor.
    pub factor: f64,
    /// Vendor reason code.
    pub reason: u32,
    /// Shareholder option number.
    pub option: u32,
    /// Record publication time.
    pub ts_created: String,
}

/// Provenance for a separately materialized adjusted OHLCV cache.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DatabentoAdjustedManifest {
    /// Manifest schema version.
    pub version: u32,
    /// Immutable raw CSV input.
    pub raw_cache_path: String,
    /// SHA-256 of the raw CSV.
    pub raw_sha256: String,
    /// SHA-256 of the persisted factor response.
    pub adjustment_factors_sha256: String,
    /// SHA-256 of the adjusted CSV.
    pub adjusted_sha256: String,
    /// Materialized row count.
    pub source_row_count: usize,
    /// Explicit adjustment policy.
    pub adjustment_mode: String,
    /// All returned factor statuses, including pending/rescinded records.
    pub factors: Vec<AdjustmentFactorRecord>,
}

/// Engine-relevant point-in-time fields mapped from a Databento definition record.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DatabentoInstrumentDefinition {
    /// Capture time of this definition version.
    pub ts_recv: String,
    /// Publisher and numeric instrument identity.
    pub publisher_id: u16,
    /// Publisher instrument id.
    pub instrument_id: u32,
    /// Publisher raw symbol.
    pub raw_symbol: String,
    /// Engine asset class.
    pub asset_class: String,
    /// Quote/settlement currency.
    pub currency: String,
    /// Minimum price increment.
    pub tick_size: String,
    /// Minimum round-lot quantity.
    pub lot_size: String,
    /// Economic contract multiplier for derivatives.
    pub contract_multiplier: Option<String>,
    /// Last eligible trade time.
    pub expiration: Option<String>,
    /// `call` or `put`.
    pub option_kind: Option<String>,
    /// Option strike.
    pub option_strike: Option<String>,
    /// Publisher raw underlying symbol.
    pub option_underlying: Option<String>,
    /// `add`, `modify`, or `delete`.
    pub update_action: String,
}

/// Load the unique active definition matching `symbol`.
pub fn load_definition_for_symbol(
    path: &Path,
    symbol: &str,
) -> Result<DatabentoInstrumentDefinition> {
    let records: Vec<DatabentoInstrumentDefinition> = serde_json::from_reader(File::open(path)?)?;
    let mut matching: Vec<_> = records
        .into_iter()
        .filter(|record| record.raw_symbol == symbol)
        .collect();
    matching.sort_by(|left, right| left.ts_recv.cmp(&right.ts_recv));
    let mut active = None;
    for record in matching {
        if record.update_action == "delete" {
            active = None;
        } else {
            active = Some(record);
        }
    }
    active.ok_or_else(|| {
        Error::Invalid(format!(
            "no active point-in-time definition found for raw symbol '{symbol}'"
        ))
    })
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
    /// Definition request estimate when definition import was requested.
    pub definition_estimated_cost_usd: Option<f64>,
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
    let raw = raw_cache_path(cfg);
    if cfg.adjustment_mode == AdjustmentMode::Raw {
        raw
    } else {
        raw.with_file_name(format!(
            "{}_{}.csv",
            raw.file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("databento"),
            cfg.adjustment_mode.as_str()
        ))
    }
}

fn raw_cache_path(cfg: &DatabentoFetchConfig) -> PathBuf {
    cfg.cache_dir.join(format!(
        "{}_{}_{}_{}_{}.{}",
        sanitize_segment(&cfg.dataset),
        sanitize_segment(&cfg.symbol),
        cfg.schema.as_str(),
        cache_datetime(cfg.start),
        cache_datetime(cfg.end),
        if cfg.schema.is_ohlcv() {
            "csv"
        } else {
            "jsonl"
        }
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

fn raw_manifest_path(cfg: &DatabentoFetchConfig) -> PathBuf {
    raw_cache_path(cfg).with_extension("manifest.json")
}

fn factors_path(cfg: &DatabentoFetchConfig) -> PathBuf {
    raw_cache_path(cfg).with_extension("factors.json")
}

fn definitions_path(cfg: &DatabentoFetchConfig) -> PathBuf {
    raw_cache_path(cfg).with_extension("definitions.json")
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
    if !cfg.schema.is_ohlcv() && cfg.adjustment_mode != AdjustmentMode::Raw {
        return Err(Error::Invalid(
            "adjustment policies apply only to OHLCV schemas".into(),
        ));
    }

    let raw_path = raw_cache_path(cfg);
    let path = cache_path(cfg);
    let raw_valid = !cfg.refresh_data && cached_request_is_valid(cfg, &raw_path)?;
    let definitions_ready = !cfg.import_definitions || definitions_path(cfg).is_file();
    if raw_valid
        && definitions_ready
        && cfg.adjustment_mode != AdjustmentMode::Raw
        && !cfg.estimate_only
        && adjusted_request_is_valid(cfg, &raw_path, &path)?
    {
        return Ok(DatabentoCacheResult {
            cache_path: path,
            estimated_cost_usd: None,
            fetched: false,
            definitions_path: cfg.import_definitions.then(|| definitions_path(cfg)),
        });
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| Error::Invalid(format!("databento async runtime failure: {err}")))?;
    let mut estimated_cost_usd = None;
    if !raw_valid || cfg.estimate_only {
        dotenvy::dotenv().ok();
        require_api_key()?;
        let inspection = rt.block_on(inspect_request(cfg))?;
        estimated_cost_usd = Some(inspection.estimated_cost_usd);
        println!(
            "Databento estimated cost: ${:.6}; cache: {}",
            inspection.estimated_cost_usd,
            path.display()
        );

        if cfg.estimate_only {
            write_json_atomic(&inspection_path(cfg), &inspection)?;
            return Ok(DatabentoCacheResult {
                cache_path: path,
                estimated_cost_usd,
                fetched: false,
                definitions_path: None,
            });
        }

        confirm_cost(cfg, inspection.estimated_cost_usd)?;
        let source_row_count = rt.block_on(fetch_to_cache(cfg, &raw_path))?;
        let manifest = cache_manifest(cfg, &raw_path, source_row_count)?;
        write_json_atomic(&raw_manifest_path(cfg), &manifest)?;
    }

    if cfg.import_definitions && (!definitions_ready || cfg.refresh_data) {
        dotenvy::dotenv().ok();
        require_api_key()?;
        let definitions = rt.block_on(fetch_definitions(cfg))?;
        write_json_atomic(&definitions_path(cfg), &definitions)?;
    }

    if cfg.adjustment_mode == AdjustmentMode::Raw {
        return Ok(DatabentoCacheResult {
            cache_path: raw_path,
            estimated_cost_usd,
            fetched: !raw_valid,
            definitions_path: cfg.import_definitions.then(|| definitions_path(cfg)),
        });
    }

    dotenvy::dotenv().ok();
    require_api_key()?;
    let factors = if !cfg.refresh_data && factors_path(cfg).is_file() {
        serde_json::from_reader(File::open(factors_path(cfg))?)?
    } else {
        let factors = rt.block_on(fetch_adjustment_factors(cfg))?;
        write_json_atomic(&factors_path(cfg), &factors)?;
        factors
    };
    let source_row_count =
        materialize_adjusted_csv(&raw_path, &path, &factors, cfg.adjustment_mode)?;
    let manifest = adjusted_manifest(cfg, &raw_path, &path, source_row_count, factors)?;
    write_json_atomic(&manifest_path(cfg), &manifest)?;

    Ok(DatabentoCacheResult {
        cache_path: path,
        estimated_cost_usd,
        fetched: true,
        definitions_path: cfg.import_definitions.then(|| definitions_path(cfg)),
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
    let market_data_cost_usd = client
        .metadata()
        .get_cost(&params)
        .await
        .map_err(map_api_error)?;
    let definition_estimated_cost_usd = if cfg.import_definitions {
        let definition_range = range
            .range_by_schema
            .get(&Schema::Definition)
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "databento dataset '{}' does not advertise point-in-time definitions",
                    cfg.dataset
                ))
            })?;
        if cfg.start < definition_range.start || cfg.end > definition_range.end {
            return Err(Error::Invalid(format!(
                "databento definition request {}..{} falls outside entitled range {}..{}",
                cfg.start, cfg.end, definition_range.start, definition_range.end
            )));
        }
        let definition_params = GetCostParams::builder()
            .dataset(&cfg.dataset)
            .symbols(cfg.symbol.as_str())
            .schema(Schema::Definition)
            .date_time_range(cfg.start..cfg.end)
            .stype_in(cfg.stype_in.as_dbn())
            .build();
        Some(
            client
                .metadata()
                .get_cost(&definition_params)
                .await
                .map_err(map_api_error)?,
        )
    } else {
        None
    };
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
        definition_estimated_cost_usd,
        estimated_cost_usd: market_data_cost_usd
            + definition_estimated_cost_usd.unwrap_or_default(),
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
    if cfg.schema.is_ohlcv() {
        fetch_ohlcv_to_cache(cfg, path).await
    } else {
        fetch_events_to_cache(cfg, path).await
    }
}

async fn fetch_ohlcv_to_cache(cfg: &DatabentoFetchConfig, path: &Path) -> Result<usize> {
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

async fn fetch_events_to_cache(cfg: &DatabentoFetchConfig, path: &Path) -> Result<usize> {
    use databento::dbn::{ImbalanceMsg, Mbp10Msg, Mbp1Msg, StatusMsg, TradeMsg};

    let parent = path.parent().ok_or_else(|| {
        Error::Invalid(format!(
            "invalid databento cache path '{}': missing parent directory",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent)?;
    let tmp_path = path.with_extension("jsonl.tmp");
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
    let mut writer = BufWriter::new(File::create(&tmp_path)?);
    let instrument = crate::types::InstrumentId::new("databento", cfg.symbol.clone());
    let mut rows = 0usize;
    macro_rules! decode {
        ($record:ty, $convert:expr) => {
            while let Some(record) = decoder.decode_record::<$record>().await.map_err(|error| {
                Error::Invalid(format!(
                    "malformed Databento {} record: {error}",
                    cfg.schema.as_str()
                ))
            })? {
                if let Some(event) = $convert(record, cfg, &instrument)? {
                    serde_json::to_writer(&mut writer, &event)?;
                    writer.write_all(b"\n")?;
                    rows += 1;
                }
            }
        };
    }
    match cfg.schema {
        DatabentoOhlcvSchema::Trades => decode!(TradeMsg, trade_event),
        DatabentoOhlcvSchema::Mbp1 => decode!(Mbp1Msg, mbp1_event),
        DatabentoOhlcvSchema::Mbp10 => decode!(Mbp10Msg, mbp10_event),
        DatabentoOhlcvSchema::Status => decode!(StatusMsg, status_event),
        DatabentoOhlcvSchema::Imbalance => decode!(ImbalanceMsg, imbalance_event),
        _ => unreachable!("OHLCV handled by fetch_ohlcv_to_cache"),
    }
    if rows == 0 {
        let _ = fs::remove_file(&tmp_path);
        return Err(Error::Invalid(format!(
            "malformed databento data: decoded zero {} events",
            cfg.schema.as_str()
        )));
    }
    writer.flush()?;
    drop(writer);
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(tmp_path, path)?;
    Ok(rows)
}

fn trade_event(
    record: &databento::dbn::TradeMsg,
    cfg: &DatabentoFetchConfig,
    instrument: &crate::types::InstrumentId,
) -> Result<Option<crate::events::Event>> {
    Ok(Some(crate::events::Event::Market(
        crate::events::MarketEvent::Trade {
            instrument: instrument.clone(),
            ts: event_timestamp(&record.hd)?,
            price: required_dbn_price(record.price, "trade price")?,
            qty: Decimal::from(record.size),
            provenance: Some(feed_provenance(
                cfg,
                &record.hd,
                record.ts_recv,
                Some(u64::from(record.sequence)),
            )?),
        },
    )))
}

fn mbp1_event(
    record: &databento::dbn::Mbp1Msg,
    cfg: &DatabentoFetchConfig,
    instrument: &crate::types::InstrumentId,
) -> Result<Option<crate::events::Event>> {
    let level = &record.levels[0];
    let (Some(bid), Some(ask)) = (
        optional_dbn_price(level.bid_px),
        optional_dbn_price(level.ask_px),
    ) else {
        return Ok(None);
    };
    Ok(Some(crate::events::Event::Market(
        crate::events::MarketEvent::BookL1 {
            instrument: instrument.clone(),
            ts: event_timestamp(&record.hd)?,
            bid,
            ask,
            provenance: Some(feed_provenance(
                cfg,
                &record.hd,
                record.ts_recv,
                Some(u64::from(record.sequence)),
            )?),
        },
    )))
}

fn mbp10_event(
    record: &databento::dbn::Mbp10Msg,
    cfg: &DatabentoFetchConfig,
    instrument: &crate::types::InstrumentId,
) -> Result<Option<crate::events::Event>> {
    let bids = record
        .levels
        .iter()
        .filter_map(|level| {
            optional_dbn_price(level.bid_px).map(|price| (price, Decimal::from(level.bid_sz)))
        })
        .collect();
    let asks = record
        .levels
        .iter()
        .filter_map(|level| {
            optional_dbn_price(level.ask_px).map(|price| (price, Decimal::from(level.ask_sz)))
        })
        .collect();
    Ok(Some(crate::events::Event::Market(
        crate::events::MarketEvent::BookL2Snapshot(crate::events::BookL2Snapshot {
            instrument: instrument.clone(),
            ts: event_timestamp(&record.hd)?,
            bids,
            asks,
            provenance: Some(feed_provenance(
                cfg,
                &record.hd,
                record.ts_recv,
                Some(u64::from(record.sequence)),
            )?),
        }),
    )))
}

fn status_event(
    record: &databento::dbn::StatusMsg,
    cfg: &DatabentoFetchConfig,
    instrument: &crate::types::InstrumentId,
) -> Result<Option<crate::events::Event>> {
    Ok(Some(crate::events::Event::Market(
        crate::events::MarketEvent::Status(crate::events::MarketStatusEvent {
            instrument: instrument.clone(),
            ts: event_timestamp(&record.hd)?,
            action: record.action,
            reason: record.reason,
            trading_event: record.trading_event,
            is_trading: record.is_trading(),
            is_quoting: record.is_quoting(),
            is_short_sell_restricted: record.is_short_sell_restricted(),
            provenance: feed_provenance(cfg, &record.hd, record.ts_recv, None)?,
        }),
    )))
}

fn imbalance_event(
    record: &databento::dbn::ImbalanceMsg,
    cfg: &DatabentoFetchConfig,
    instrument: &crate::types::InstrumentId,
) -> Result<Option<crate::events::Event>> {
    Ok(Some(crate::events::Event::Market(
        crate::events::MarketEvent::AuctionImbalance(crate::events::AuctionImbalanceEvent {
            instrument: instrument.clone(),
            ts: event_timestamp(&record.hd)?,
            reference_price: optional_dbn_price(record.ref_price),
            indicative_match_price: optional_dbn_price(record.ind_match_price),
            paired_qty: optional_dbn_qty(record.paired_qty),
            total_imbalance_qty: optional_dbn_qty(record.total_imbalance_qty),
            side: dbn_char(record.side),
            auction_type: dbn_char(record.auction_type),
            auction_status: record.auction_status,
            provenance: feed_provenance(cfg, &record.hd, record.ts_recv, None)?,
        }),
    )))
}

fn feed_provenance(
    cfg: &DatabentoFetchConfig,
    header: &databento::dbn::RecordHeader,
    ts_recv: u64,
    sequence: Option<u64>,
) -> Result<crate::events::MarketDataProvenance> {
    Ok(crate::events::MarketDataProvenance {
        dataset: cfg.dataset.clone(),
        publisher_id: header.publisher_id,
        instrument_id: header.instrument_id,
        ts_recv: optional_dbn_timestamp(ts_recv)?,
        sequence,
    })
}

fn event_timestamp(header: &databento::dbn::RecordHeader) -> Result<OffsetDateTime> {
    header
        .ts_event()
        .ok_or_else(|| Error::Invalid("malformed Databento event: missing ts_event".into()))
}

fn optional_dbn_timestamp(value: u64) -> Result<Option<OffsetDateTime>> {
    if value == databento::dbn::UNDEF_TIMESTAMP {
        return Ok(None);
    }
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(value))
        .map(Some)
        .map_err(|error| Error::Invalid(format!("malformed Databento timestamp: {error}")))
}

fn required_dbn_price(value: i64, field: &str) -> Result<Decimal> {
    optional_dbn_price(value)
        .ok_or_else(|| Error::Invalid(format!("malformed Databento event: missing {field}")))
}

fn optional_dbn_price(value: i64) -> Option<Decimal> {
    (value != databento::dbn::UNDEF_PRICE)
        .then(|| Decimal::from_i128_with_scale(i128::from(value), 9))
}

fn optional_dbn_qty(value: u32) -> Option<u32> {
    (value != databento::dbn::UNDEF_ORDER_SIZE).then_some(value)
}

fn dbn_char(value: std::os::raw::c_char) -> Option<String> {
    let value = value as u8 as char;
    (!matches!(value, '\0' | '~')).then(|| value.to_string())
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
    let manifest_path = raw_manifest_path(cfg);
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

fn adjusted_request_is_valid(
    cfg: &DatabentoFetchConfig,
    raw_path: &Path,
    adjusted_path: &Path,
) -> Result<bool> {
    let factor_path = factors_path(cfg);
    let manifest_path = manifest_path(cfg);
    if !adjusted_path.is_file() || !factor_path.is_file() || !manifest_path.is_file() {
        return Ok(false);
    }
    let manifest: DatabentoAdjustedManifest =
        match serde_json::from_reader(File::open(manifest_path)?) {
            Ok(manifest) => manifest,
            Err(_) => return Ok(false),
        };
    Ok(manifest.version == 1
        && manifest.adjustment_mode == cfg.adjustment_mode.as_str()
        && manifest.raw_sha256 == sha256_file(raw_path)?
        && manifest.adjustment_factors_sha256 == sha256_file(&factor_path)?
        && manifest.adjusted_sha256 == sha256_file(adjusted_path)?)
}

async fn fetch_adjustment_factors(
    cfg: &DatabentoFetchConfig,
) -> Result<Vec<AdjustmentFactorRecord>> {
    use databento::reference::{adjustment, AdjustmentStatus};

    let mut client = ReferenceClient::builder()
        .key_from_env()
        .map_err(|err| Error::Invalid(format!("databento missing API key: {err}")))?
        .build()
        .map_err(map_api_error)?;
    let params = adjustment::GetRangeParams::builder()
        .symbols(cfg.symbol.as_str())
        .stype_in(cfg.stype_in.as_dbn())
        .start(cfg.start)
        .end(cfg.end)
        .build();
    client
        .adjustment_factors()
        .get_range(&params)
        .await
        .map_err(map_api_error)?
        .into_iter()
        .map(|factor| {
            Ok(AdjustmentFactorRecord {
                event_id: factor.event_id,
                ex_date: factor.ex_date.to_string(),
                status: match factor.status {
                    AdjustmentStatus::Apply => "apply",
                    AdjustmentStatus::Pending => "pending",
                    AdjustmentStatus::Rescind => "rescind",
                }
                .into(),
                factor: factor.factor,
                reason: factor.reason,
                option: factor.option,
                ts_created: format_utc(factor.ts_created)?,
            })
        })
        .collect()
}

async fn fetch_definitions(
    cfg: &DatabentoFetchConfig,
) -> Result<Vec<DatabentoInstrumentDefinition>> {
    use databento::dbn::InstrumentDefMsg;

    let mut client = client_from_env()?;
    let params = GetRangeParams::builder()
        .dataset(&cfg.dataset)
        .symbols(cfg.symbol.as_str())
        .schema(Schema::Definition)
        .date_time_range(cfg.start..cfg.end)
        .stype_in(cfg.stype_in.as_dbn())
        .build();
    let mut decoder = client
        .timeseries()
        .get_range(&params)
        .await
        .map_err(map_api_error)?;
    let mut definitions = Vec::new();
    while let Some(record) = decoder
        .decode_record::<InstrumentDefMsg>()
        .await
        .map_err(|error| {
            Error::Invalid(format!("malformed databento definition record: {error}"))
        })?
    {
        definitions.push(map_definition(record)?);
    }
    if definitions.is_empty() {
        return Err(Error::Invalid(format!(
            "databento returned no point-in-time definitions for '{}'",
            cfg.symbol
        )));
    }
    Ok(definitions)
}

fn map_definition(
    record: &databento::dbn::InstrumentDefMsg,
) -> Result<DatabentoInstrumentDefinition> {
    use databento::dbn::{InstrumentClass, SecurityUpdateAction};

    let instrument_class = record.instrument_class().map_err(map_dbn_error)?;
    let asset_class = match instrument_class {
        InstrumentClass::Stock => "equity",
        InstrumentClass::Future => "future",
        InstrumentClass::Call | InstrumentClass::Put => "option",
        InstrumentClass::Bond => "bond",
        InstrumentClass::FxSpot => "forex",
        other => {
            return Err(Error::Invalid(format!(
                "unsupported Databento instrument class {other:?}"
            )))
        }
    };
    let raw_symbol = nonempty_definition(record.raw_symbol(), "raw_symbol")?;
    let currency = nonempty_definition(record.currency(), "currency")?;
    let tick_size = positive_fixed(record.min_price_increment, "min_price_increment")?;
    let round_lot = if record.min_lot_size_round_lot > 0 {
        record.min_lot_size_round_lot
    } else {
        record.min_lot_size
    };
    if round_lot <= 0 {
        return Err(Error::Invalid(format!(
            "unsupported instrument '{raw_symbol}': missing positive round lot"
        )));
    }
    let derivative = matches!(
        instrument_class,
        InstrumentClass::Future | InstrumentClass::Call | InstrumentClass::Put
    );
    let contract_multiplier = derivative
        .then(|| {
            if record.unit_of_measure_qty != databento::dbn::UNDEF_PRICE
                && record.unit_of_measure_qty > 0
            {
                positive_fixed(record.unit_of_measure_qty, "unit_of_measure_qty")
            } else if record.contract_multiplier > 0 {
                Ok(record.contract_multiplier.to_string())
            } else {
                Err(Error::Invalid(format!(
                    "unsupported derivative '{raw_symbol}': missing contract multiplier"
                )))
            }
        })
        .transpose()?;
    let expiration = derivative
        .then(|| {
            record.expiration().ok_or_else(|| {
                Error::Invalid(format!(
                    "unsupported derivative '{raw_symbol}': missing expiration"
                ))
            })
        })
        .transpose()?
        .map(format_utc)
        .transpose()?;
    let option_kind = match instrument_class {
        InstrumentClass::Call => Some("call".into()),
        InstrumentClass::Put => Some("put".into()),
        _ => None,
    };
    let option_strike = option_kind
        .as_ref()
        .map(|_| positive_fixed(record.strike_price, "strike_price"))
        .transpose()?;
    let option_underlying = option_kind
        .as_ref()
        .map(|_| nonempty_definition(record.underlying(), "underlying"))
        .transpose()?;
    let update_action = match record.security_update_action().map_err(map_dbn_error)? {
        SecurityUpdateAction::Add => "add",
        SecurityUpdateAction::Modify => "modify",
        SecurityUpdateAction::Delete => "delete",
        #[allow(deprecated)]
        SecurityUpdateAction::Invalid => {
            return Err(Error::Invalid(format!(
                "invalid definition update action for '{raw_symbol}'"
            )))
        }
    };
    Ok(DatabentoInstrumentDefinition {
        ts_recv: format_utc(record.ts_recv().ok_or_else(|| {
            Error::Invalid(format!("definition '{raw_symbol}' missing ts_recv"))
        })?)?,
        publisher_id: record.hd.publisher_id,
        instrument_id: record.hd.instrument_id,
        raw_symbol,
        asset_class: asset_class.into(),
        currency,
        tick_size,
        lot_size: round_lot.to_string(),
        contract_multiplier,
        expiration,
        option_kind,
        option_strike,
        option_underlying,
        update_action: update_action.into(),
    })
}

fn nonempty_definition(
    value: std::result::Result<&str, databento::dbn::Error>,
    field: &str,
) -> Result<String> {
    let value = value.map_err(map_dbn_error)?.trim();
    if value.is_empty() {
        Err(Error::Invalid(format!(
            "unsupported instrument definition: missing {field}"
        )))
    } else {
        Ok(value.into())
    }
}

fn positive_fixed(value: i64, field: &str) -> Result<String> {
    if value == databento::dbn::UNDEF_PRICE || value <= 0 {
        Err(Error::Invalid(format!(
            "unsupported instrument definition: missing positive {field}"
        )))
    } else {
        Ok(format_fixed_price(value))
    }
}

fn map_dbn_error(error: databento::dbn::Error) -> Error {
    Error::Invalid(format!("malformed Databento definition: {error}"))
}

fn adjusted_manifest(
    cfg: &DatabentoFetchConfig,
    raw_path: &Path,
    adjusted_path: &Path,
    source_row_count: usize,
    factors: Vec<AdjustmentFactorRecord>,
) -> Result<DatabentoAdjustedManifest> {
    Ok(DatabentoAdjustedManifest {
        version: 1,
        raw_cache_path: raw_path.display().to_string(),
        raw_sha256: sha256_file(raw_path)?,
        adjustment_factors_sha256: sha256_file(&factors_path(cfg))?,
        adjusted_sha256: sha256_file(adjusted_path)?,
        source_row_count,
        adjustment_mode: cfg.adjustment_mode.as_str().into(),
        factors,
    })
}

#[derive(Deserialize, Serialize)]
struct CsvBar {
    ts: String,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
}

fn materialize_adjusted_csv(
    raw_path: &Path,
    adjusted_path: &Path,
    factors: &[AdjustmentFactorRecord],
    mode: AdjustmentMode,
) -> Result<usize> {
    let active = active_factors(factors, mode)?;
    let tmp_path = adjusted_path.with_extension("csv.tmp");
    let mut reader = csv::Reader::from_path(raw_path).map_err(csv_io)?;
    let mut writer = csv::Writer::from_path(&tmp_path).map_err(csv_io)?;
    let mut rows = 0usize;
    for row in reader.deserialize::<CsvBar>() {
        let mut row = row.map_err(csv_io)?;
        let ts = OffsetDateTime::parse(&row.ts, &Rfc3339)
            .map_err(|error| Error::Invalid(format!("invalid raw OHLCV timestamp: {error}")))?;
        let mut price_factor = Decimal::ONE;
        let mut volume_factor = Decimal::ONE;
        // Adjustment lists are small. If this becomes a hotspot, replace the scan with a
        // reverse-date cumulative-factor cursor.
        for factor in active.iter().filter(|factor| ts.date() < factor.ex_date) {
            price_factor *= factor.factor;
            if matches!(factor.reason, 5 | 6) {
                volume_factor *= factor.factor;
            }
        }
        row.open = adjusted_decimal(&row.open, price_factor, "open")?;
        row.high = adjusted_decimal(&row.high, price_factor, "high")?;
        row.low = adjusted_decimal(&row.low, price_factor, "low")?;
        row.close = adjusted_decimal(&row.close, price_factor, "close")?;
        let volume = parse_decimal(&row.volume, "volume")?;
        row.volume = (volume / volume_factor).normalize().to_string();
        writer.serialize(row).map_err(csv_io)?;
        rows += 1;
    }
    writer.flush().map_err(Error::Io)?;
    drop(writer);
    if adjusted_path.exists() {
        fs::remove_file(adjusted_path)?;
    }
    fs::rename(tmp_path, adjusted_path)?;
    Ok(rows)
}

struct ActiveFactor {
    ex_date: Date,
    factor: Decimal,
    reason: u32,
}

fn active_factors(
    factors: &[AdjustmentFactorRecord],
    mode: AdjustmentMode,
) -> Result<Vec<ActiveFactor>> {
    let mut ordered = factors.to_vec();
    ordered.sort_by(|left, right| left.ts_created.cmp(&right.ts_created));
    let mut active = BTreeMap::new();
    for factor in ordered {
        if factor.option != 1 || !mode.accepts_reason(factor.reason) {
            continue;
        }
        let key = (
            factor.event_id.clone(),
            factor.ex_date.clone(),
            factor.option,
            factor.reason,
        );
        match factor.status.as_str() {
            "apply" => {
                if !factor.factor.is_finite() || factor.factor <= 0.0 {
                    return Err(Error::Invalid(format!(
                        "unsupported nonpositive adjustment factor {} for event {}",
                        factor.factor, factor.event_id
                    )));
                }
                active.insert(key, factor);
            }
            "rescind" => {
                active.remove(&key);
            }
            "pending" => {}
            status => {
                return Err(Error::Invalid(format!(
                    "unknown adjustment factor status '{status}'"
                )))
            }
        }
    }
    let date_format = format_description!("[year]-[month]-[day]");
    active
        .into_values()
        .map(|factor| {
            Ok(ActiveFactor {
                ex_date: Date::parse(&factor.ex_date, &date_format).map_err(|error| {
                    Error::Invalid(format!(
                        "invalid adjustment ex-date '{}': {error}",
                        factor.ex_date
                    ))
                })?,
                factor: Decimal::from_f64(factor.factor).ok_or_else(|| {
                    Error::Invalid(format!("invalid adjustment factor {}", factor.factor))
                })?,
                reason: factor.reason,
            })
        })
        .collect()
}

fn adjusted_decimal(value: &str, factor: Decimal, field: &str) -> Result<String> {
    Ok((parse_decimal(value, field)? * factor)
        .normalize()
        .to_string())
}

fn parse_decimal(value: &str, field: &str) -> Result<Decimal> {
    Decimal::from_str(value)
        .map_err(|error| Error::Invalid(format!("invalid raw OHLCV {field} '{value}': {error}")))
}

fn csv_io(error: csv::Error) -> Error {
    Error::Io(io::Error::new(io::ErrorKind::InvalidData, error))
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
            adjustment_mode: AdjustmentMode::Raw,
            import_definitions: false,
        }
    }

    #[test]
    fn parses_only_ohlcv_schemas() {
        assert_eq!(
            DatabentoOhlcvSchema::parse("ohlcv-1d").unwrap(),
            DatabentoOhlcvSchema::Ohlcv1D
        );
        assert_eq!(
            DatabentoOhlcvSchema::parse("trades").unwrap(),
            DatabentoOhlcvSchema::Trades
        );
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

    #[test]
    fn split_adjustment_is_separate_and_factor_changes_invalidate_it() {
        let mut config = cfg();
        config.adjustment_mode = AdjustmentMode::SplitAdjusted;
        let root = std::env::temp_dir().join(format!("pallas-adjusted-{}", uuid::Uuid::new_v4()));
        config.cache_dir = root.clone();
        std::fs::create_dir_all(&root).unwrap();
        let raw = raw_cache_path(&config);
        std::fs::write(
            &raw,
            concat!(
                "ts,open,high,low,close,volume\n",
                "2025-01-01T00:00:00Z,100,110,90,105,10\n",
                "2025-01-03T00:00:00Z,50,55,45,52.5,20\n"
            ),
        )
        .unwrap();
        let factors = vec![AdjustmentFactorRecord {
            event_id: "split-1".into(),
            ex_date: "2025-01-02".into(),
            status: "apply".into(),
            factor: 0.5,
            reason: 5,
            option: 1,
            ts_created: "2025-01-01T20:00:00Z".into(),
        }];
        write_json_atomic(&factors_path(&config), &factors).unwrap();
        let adjusted = cache_path(&config);
        let rows =
            materialize_adjusted_csv(&raw, &adjusted, &factors, AdjustmentMode::SplitAdjusted)
                .unwrap();
        let manifest = adjusted_manifest(&config, &raw, &adjusted, rows, factors.clone()).unwrap();
        write_json_atomic(&manifest_path(&config), &manifest).unwrap();

        let mut reader = csv::Reader::from_path(&adjusted).unwrap();
        let bars: Vec<CsvBar> = reader.deserialize().map(|row| row.unwrap()).collect();
        assert_eq!(bars[0].close, "52.5");
        assert_eq!(bars[0].volume, "20");
        assert_eq!(bars[1].close, "52.5");
        assert!(adjusted_request_is_valid(&config, &raw, &adjusted).unwrap());

        let mut rescinded = factors;
        rescinded.push(AdjustmentFactorRecord {
            event_id: "split-1".into(),
            ex_date: "2025-01-02".into(),
            status: "rescind".into(),
            factor: 0.5,
            reason: 5,
            option: 1,
            ts_created: "2025-01-04T00:00:00Z".into(),
        });
        write_json_atomic(&factors_path(&config), &rescinded).unwrap();
        assert!(!adjusted_request_is_valid(&config, &raw, &adjusted).unwrap());
        assert!(active_factors(&rescinded, AdjustmentMode::SplitAdjusted)
            .unwrap()
            .is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn futures_and_options_definitions_round_trip_without_symbol_inference() {
        use databento::dbn::record::str_to_c_chars;
        use databento::dbn::{InstrumentDefMsg, UNDEF_PRICE};
        use time::macros::datetime;

        let expiry = datetime!(2025-12-19 21:00 UTC);
        let mut future = InstrumentDefMsg::default();
        future.hd.publisher_id = 1;
        future.hd.instrument_id = 101;
        future.ts_recv = datetime!(2025-01-02 00:00 UTC).unix_timestamp_nanos() as u64;
        future.min_price_increment = 250_000_000;
        future.unit_of_measure_qty = 50_000_000_000;
        future.min_lot_size_round_lot = 1;
        future.expiration = expiry.unix_timestamp_nanos() as u64;
        future.raw_symbol = str_to_c_chars("ESZ5").unwrap();
        future.currency = str_to_c_chars("USD").unwrap();
        future.instrument_class = b'F' as _;
        future.security_update_action = b'A' as _;

        let mapped_future = map_definition(&future).unwrap();
        assert_eq!(mapped_future.asset_class, "future");
        assert_eq!(mapped_future.tick_size, "0.25");
        assert_eq!(mapped_future.contract_multiplier.as_deref(), Some("50"));

        let mut option = future.clone();
        option.hd.instrument_id = 202;
        option.raw_symbol = str_to_c_chars("AAPL  251219C00200000").unwrap();
        option.instrument_class = b'C' as _;
        option.strike_price = 200_000_000_000;
        option.unit_of_measure_qty = 100_000_000_000;
        option.underlying = str_to_c_chars("AAPL").unwrap();
        let mapped_option = map_definition(&option).unwrap();
        assert_eq!(mapped_option.option_kind.as_deref(), Some("call"));
        assert_eq!(mapped_option.option_strike.as_deref(), Some("200"));
        assert_eq!(mapped_option.option_underlying.as_deref(), Some("AAPL"));

        let json = serde_json::to_string(&[mapped_future, mapped_option]).unwrap();
        let decoded: Vec<DatabentoInstrumentDefinition> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.len(), 2);

        option.strike_price = UNDEF_PRICE;
        assert!(map_definition(&option)
            .unwrap_err()
            .to_string()
            .contains("strike_price"));
    }

    #[test]
    fn trade_records_map_to_replay_events_with_feed_identity() {
        use databento::dbn::TradeMsg;
        use time::macros::datetime;

        let mut record = TradeMsg::default();
        record.hd.publisher_id = 7;
        record.hd.instrument_id = 99;
        record.hd.ts_event = datetime!(2025-01-02 14:30 UTC).unix_timestamp_nanos() as u64;
        record.ts_recv = datetime!(2025-01-02 14:30:00.000001 UTC).unix_timestamp_nanos() as u64;
        record.price = 6_000_250_000_000;
        record.size = 3;
        record.sequence = 42;
        let mut config = cfg();
        config.dataset = "GLBX.MDP3".into();
        config.schema = DatabentoOhlcvSchema::Trades;
        let instrument = crate::types::InstrumentId::new("databento", "ESZ5");

        let crate::events::Event::Market(crate::events::MarketEvent::Trade {
            price,
            qty,
            provenance: Some(provenance),
            ..
        }) = trade_event(&record, &config, &instrument).unwrap().unwrap()
        else {
            panic!("expected normalized trade");
        };
        assert_eq!(price.to_string(), "6000.250000000");
        assert_eq!(qty, Decimal::from(3));
        assert_eq!(provenance.dataset, "GLBX.MDP3");
        assert_eq!(provenance.sequence, Some(42));
    }
}
