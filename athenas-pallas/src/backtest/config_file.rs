//! Typed TOML loading for [`BacktestConfig`].

use std::path::{Path, PathBuf};

use rust_decimal::Decimal;
use serde::Deserialize;

use super::config::{parse_asset_class, parse_data_format, BacktestConfig, ExtraInstrument};
use crate::types::{Asset, InstrumentId};

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FileConfig {
    instrument: Option<InstrumentConfig>,
    backtest: Option<RunConfig>,
    balances: Option<Vec<BalanceConfig>>,
    instruments: Option<Vec<ExtraInstrumentConfig>>,
    strategy_parameters: Option<std::collections::HashMap<String, serde_json::Value>>,
    fee_bps: Option<DecimalValue>,
    slippage_bps: Option<DecimalValue>,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct InstrumentConfig {
    exchange: Option<String>,
    symbol: Option<String>,
    asset_class: Option<String>,
    lot_size: Option<DecimalValue>,
    tick_size: Option<DecimalValue>,
    contract_multiplier: Option<DecimalValue>,
    expiry: Option<String>,
    base_asset: Option<String>,
    quote_asset: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RunConfig {
    data: Option<String>,
    data_format: Option<String>,
    fee_bps: Option<DecimalValue>,
    slippage_bps: Option<DecimalValue>,
    half_spread_bps: Option<DecimalValue>,
    buy_and_hold_qty: Option<DecimalValue>,
    periods_per_year: Option<f64>,
    bar_interval: Option<String>,
    session_filter: Option<String>,
    auto_periods_per_year: Option<bool>,
    output: Option<String>,
    strategy: Option<String>,
    python: Option<String>,
    record_equity_curve: Option<bool>,
    risk_free_annual: Option<f64>,
    max_position_abs: Option<DecimalValue>,
    max_daily_loss_quote: Option<DecimalValue>,
    margin_initial_rate: Option<DecimalValue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BalanceConfig {
    asset: String,
    amount: DecimalValue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtraInstrumentConfig {
    exchange: String,
    symbol: String,
    #[serde(default = "default_asset_class")]
    asset_class: String,
    lot_size: Option<DecimalValue>,
    tick_size: Option<DecimalValue>,
    contract_multiplier: Option<DecimalValue>,
    expiry: Option<String>,
    margin_initial_rate: Option<DecimalValue>,
    #[serde(alias = "data_path")]
    data: Option<String>,
    data_format: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DecimalValue {
    String(String),
    Integer(i64),
    Float(f64),
}

impl DecimalValue {
    fn parse(self, field: &str) -> crate::Result<Decimal> {
        let value = match self {
            Self::String(value) => value,
            Self::Integer(value) => value.to_string(),
            Self::Float(value) if value.is_finite() => value.to_string(),
            Self::Float(_) => return Err(invalid(field, "must be finite")),
        };
        value
            .parse()
            .map_err(|_| invalid(field, &format!("invalid decimal `{value}`")))
    }

    fn nonnegative(self, field: &str) -> crate::Result<Decimal> {
        let value = self.parse(field)?;
        if value < Decimal::ZERO {
            return Err(invalid(field, "must be nonnegative"));
        }
        Ok(value)
    }

    fn positive(self, field: &str) -> crate::Result<Decimal> {
        let value = self.parse(field)?;
        if value <= Decimal::ZERO {
            return Err(invalid(field, "must be positive"));
        }
        Ok(value)
    }

    fn rate(self, field: &str) -> crate::Result<Decimal> {
        let value = self.parse(field)?;
        if value <= Decimal::ZERO || value > Decimal::ONE {
            return Err(invalid(field, "must be greater than zero and at most one"));
        }
        Ok(value)
    }
}

impl BacktestConfig {
    /// Load settings from a TOML file into a new config.
    pub fn load_toml(path: &Path) -> crate::Result<Self> {
        let mut config = Self::default();
        config.apply_toml(path)?;
        Ok(config)
    }

    /// Merge TOML values into this config.
    pub fn apply_toml(&mut self, path: &Path) -> crate::Result<()> {
        let text = std::fs::read_to_string(path)?;
        let file: FileConfig =
            toml::from_str(&text).map_err(|error| crate::Error::Invalid(error.to_string()))?;
        file.apply(self, path.parent())
    }
}

impl FileConfig {
    fn apply(self, config: &mut BacktestConfig, base_dir: Option<&Path>) -> crate::Result<()> {
        if let Some(instrument) = self.instrument {
            instrument.apply(config)?;
        }
        if let Some(run) = self.backtest {
            run.apply(config, base_dir)?;
        }
        if let Some(value) = self.fee_bps {
            config.fee_bps = value.nonnegative("fee_bps")?;
        }
        if let Some(value) = self.slippage_bps {
            config.slippage_bps = value.nonnegative("slippage_bps")?;
        }
        if let Some(rows) = self.balances {
            config.balances = parse_balances(rows)?;
        }
        if let Some(rows) = self.instruments {
            config.extra_instruments = rows
                .into_iter()
                .map(|row| row.parse(base_dir))
                .collect::<crate::Result<_>>()?;
        }
        if let Some(parameters) = self.strategy_parameters {
            config.strategy_parameters = parameters;
        }
        Ok(())
    }
}

impl InstrumentConfig {
    fn apply(self, config: &mut BacktestConfig) -> crate::Result<()> {
        match (self.exchange, self.symbol) {
            (Some(exchange), Some(symbol)) => {
                config.instrument = InstrumentId::new(exchange, symbol);
            }
            (None, None) => {}
            _ => {
                return Err(invalid(
                    "instrument",
                    "exchange and symbol must be set together",
                ))
            }
        }
        if let Some(value) = self.asset_class {
            config.asset_class = parse_asset_class(&value).map_err(crate::Error::Invalid)?;
        }
        if let Some(value) = self.lot_size {
            config.lot_size = Some(value.positive("instrument.lot_size")?);
        }
        if let Some(value) = self.tick_size {
            config.tick_size = Some(value.positive("instrument.tick_size")?);
        }
        if let Some(value) = self.contract_multiplier {
            config.contract_multiplier = Some(value.positive("instrument.contract_multiplier")?);
        }
        if self.expiry.is_some() {
            config.expiry = self.expiry;
        }
        if self.base_asset.is_some() {
            config.base_asset = self.base_asset;
        }
        if self.quote_asset.is_some() {
            config.quote_asset = self.quote_asset;
        }
        Ok(())
    }
}

impl RunConfig {
    fn apply(self, config: &mut BacktestConfig, base_dir: Option<&Path>) -> crate::Result<()> {
        apply_timing(config, &self)?;
        if let Some(path) = self.data {
            config.data_path = resolve_path(base_dir, &path);
        }
        if let Some(value) = self.data_format {
            config.data_format = parse_data_format(&value).map_err(crate::Error::Invalid)?;
        }
        apply_costs(
            config,
            self.fee_bps,
            self.slippage_bps,
            self.half_spread_bps,
        )?;
        if let Some(value) = self.buy_and_hold_qty {
            config.buy_and_hold_qty = Some(value.positive("backtest.buy_and_hold_qty")?);
        }
        apply_paths(config, base_dir, self.output, self.strategy, self.python);
        if let Some(value) = self.record_equity_curve {
            config.record_equity_curve = value;
        }
        if let Some(value) = self.risk_free_annual {
            if !value.is_finite() {
                return Err(invalid("backtest.risk_free_annual", "must be finite"));
            }
            config.risk_free_annual = value;
        }
        if let Some(value) = self.max_position_abs {
            config.max_position_abs = Some(value.positive("backtest.max_position_abs")?);
        }
        if let Some(value) = self.max_daily_loss_quote {
            config.max_daily_loss_quote = Some(value.positive("backtest.max_daily_loss_quote")?);
        }
        if let Some(value) = self.margin_initial_rate {
            config.margin_initial_rate = Some(value.rate("backtest.margin_initial_rate")?);
        }
        Ok(())
    }
}

impl ExtraInstrumentConfig {
    fn parse(self, base_dir: Option<&Path>) -> crate::Result<ExtraInstrument> {
        Ok(ExtraInstrument {
            instrument: InstrumentId::new(self.exchange, self.symbol),
            asset_class: parse_asset_class(&self.asset_class).map_err(crate::Error::Invalid)?,
            lot_size: parse_positive(self.lot_size, "instruments.lot_size")?,
            tick_size: parse_positive(self.tick_size, "instruments.tick_size")?,
            contract_multiplier: parse_positive(
                self.contract_multiplier,
                "instruments.contract_multiplier",
            )?,
            expiry: self.expiry,
            margin_initial_rate: self
                .margin_initial_rate
                .map(|value| value.rate("instruments.margin_initial_rate"))
                .transpose()?,
            data_path: self.data.map(|path| resolve_path(base_dir, &path)),
            data_format: self
                .data_format
                .map(|value| parse_data_format(&value).map_err(crate::Error::Invalid))
                .transpose()?,
        })
    }
}

fn apply_costs(
    config: &mut BacktestConfig,
    fee: Option<DecimalValue>,
    slippage: Option<DecimalValue>,
    spread: Option<DecimalValue>,
) -> crate::Result<()> {
    if let Some(value) = fee {
        config.fee_bps = value.nonnegative("backtest.fee_bps")?;
    }
    if let Some(value) = slippage {
        config.slippage_bps = value.nonnegative("backtest.slippage_bps")?;
    }
    if let Some(value) = spread {
        config.half_spread_bps = value.nonnegative("backtest.half_spread_bps")?;
    }
    Ok(())
}

fn apply_timing(config: &mut BacktestConfig, run: &RunConfig) -> crate::Result<()> {
    if let Some(value) = run.periods_per_year {
        if !value.is_finite() || value <= 0.0 {
            return Err(invalid(
                "backtest.periods_per_year",
                "must be positive and finite",
            ));
        }
        config.periods_per_year = value;
        config.auto_periods_per_year = false;
    }
    if let Some(value) = &run.bar_interval {
        config.bar_interval = Some(value.clone());
    }
    if let Some(value) = &run.session_filter {
        config.session_filter = Some(value.clone());
    }
    if let Some(value) = run.auto_periods_per_year {
        config.auto_periods_per_year = value;
    }
    Ok(())
}

fn apply_paths(
    config: &mut BacktestConfig,
    base_dir: Option<&Path>,
    output: Option<String>,
    strategy: Option<String>,
    python: Option<String>,
) {
    if let Some(path) = output {
        config.output_path = Some(resolve_path(base_dir, &path));
    }
    if let Some(path) = strategy {
        config.strategy_path = Some(resolve_path(base_dir, &path));
    }
    if let Some(value) = python {
        config.python_exe = value;
    }
}

fn parse_balances(
    rows: Vec<BalanceConfig>,
) -> crate::Result<std::collections::HashMap<Asset, Decimal>> {
    rows.into_iter()
        .map(|row| {
            let amount = row.amount.nonnegative("balances.amount")?;
            Ok((Asset::new(row.asset), amount))
        })
        .collect()
}

fn parse_positive(value: Option<DecimalValue>, field: &str) -> crate::Result<Option<Decimal>> {
    value.map(|value| value.positive(field)).transpose()
}

fn resolve_path(base_dir: Option<&Path>, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else if let Some(base) = base_dir {
        base.join(path)
    } else {
        path
    }
}

fn invalid(field: &str, message: &str) -> crate::Error {
    crate::Error::Invalid(format!("{field} {message}"))
}

fn default_asset_class() -> String {
    "crypto".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_fields() {
        let error = toml::from_str::<FileConfig>("[backtest]\nfeee_bps = 1")
            .err()
            .expect("unknown field should fail");
        assert!(error.to_string().contains("feee_bps"));
    }

    #[test]
    fn rejects_invalid_decimal() {
        let value = DecimalValue::String("ten".into());
        assert!(value.parse("fee_bps").is_err());
    }
}
