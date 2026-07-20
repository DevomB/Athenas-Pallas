//! # Athena's Pallas
//!
//! Event-driven **backtest** engine with optional external C++/Python strategies.
//!
//! ## Extension
//! - Implement [`strategy::Strategy`] for in-process logic.
//! - Run Python/C++ strategies via newline-delimited JSON ([`strategy::ExternalStrategy`]).
//! - Customize fill simulation via [`execution::PaperConfig`] and [`execution::PaperExecution`].

#![warn(missing_docs)]

pub mod backtest;
mod bar;
pub mod calendar;
#[cfg(feature = "databento")]
pub mod data;
pub mod engine;
pub mod error;
pub mod events;
pub mod execution;
pub mod instrument;
mod interval;
pub mod metrics;
pub mod oms;
pub mod results;
pub mod risk;
mod source;
pub mod state;
pub mod strategy;
pub mod types;

pub use bar::{
    decimal_to_ticks, default_tick_size, parse_timestamp, ticks_to_decimal, Bar, BarSeries,
    BarSeriesSource, OhlcvRow,
};
pub use engine::{
    dispatch_event_sync, dispatch_replay_sync, dispatch_strategy_sync, replay_events_sync,
};
pub use error::{Error, Result};
pub use events::{BookL2Snapshot, Event, OrderIntentSource};
pub use risk::RiskEngine;
pub use types::{EquityPoint, ExchangeId, InstrumentId, Side, StrategyId, Symbol, TradingState};

pub use execution::{PaperConfig, PaperExecution, SyncExecutionGateway};
pub use instrument::{InstrumentIndex, InstrumentMeta, InstrumentRegistry};
pub use metrics::{
    strategy_position_report, trading_summaries_per_strategy, StrategyPositionRow, TradingSummary,
};
pub use oms::OrderStore;
pub use results::{append_results_jsonl, write_backtest_json, write_backtest_outputs};
pub use source::HistoricalSource;
pub use state::GlobalState;
pub use strategy::Strategy;
