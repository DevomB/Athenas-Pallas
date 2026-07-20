//! Historical replay, configuration, and reporting.

pub mod config;
mod config_file;
pub mod cpp_build;
pub mod lifecycle;
pub mod merge;
pub mod pbar;
pub mod replay;
pub mod report;
pub mod runner;
pub mod session;
pub mod source_loader;
pub mod sources;
pub mod strategy_resolver;

pub(crate) use crate::bar::{parse_ts, parse_ts_required_err};
pub use crate::interval::{
    default_periods_per_year, infer_periods_per_year_from_spacing, periods_per_year_from_interval,
    periods_per_year_from_interval_for_class,
};
pub use config::{
    parse_asset_class, parse_base_quote, parse_data_format, parse_instrument, BacktestConfig,
    DataFormat, ExtraInstrument,
};
pub use cpp_build::build_cpp_strategy;
pub use merge::{merge_sources_iter, MergedSources};
pub use pbar::{is_pbar_path, read_pbar, write_pbar};
pub use replay::read_events_jsonl;
pub use report::{
    BacktestParameters, BacktestReport, DataMetadata, DataSourceMetadata, FinalPosition,
    PendingOrder,
};
pub use runner::{BacktestRunner, BuyAndHold};
pub use session::{
    run_backtest, run_backtest_with_cancel, run_external_backtest,
    run_external_backtest_with_cancel,
};
pub use strategy_resolver::{detect_strategy, resolve_strategy_path, ResolvedStrategy};
