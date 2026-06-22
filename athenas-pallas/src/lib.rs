//! # Athena's Pallas
//!
//! Event-driven **backtest** engine with optional external C++/Python strategies.
//!
//! ## Extension
//! - Implement [`strategy::Strategy`] for in-process logic.
//! - Run Python/C++ strategies via newline-delimited JSON ([`strategy::ExternalStrategy`]).
//! - Customize fill simulation via [`execution::PaperConfig`] and [`execution::SyncPaperGateway`].

#![warn(missing_docs)]

pub mod backtest;
pub mod calendar;
pub mod engine;
pub mod error;
pub mod events;
pub mod execution;
pub mod instrument;
pub mod metrics;
pub mod oms;
pub mod results;
pub mod risk;
pub mod state;
pub mod strategy;
pub mod types;

pub use engine::{
    dispatch_event_sync, dispatch_replay_bar_sync, dispatch_replay_sync, dispatch_strategy_sync,
    replay_events_sync,
};
pub use error::{Error, Result};
pub use events::{BookL2Snapshot, Event, OrderIntentSource};
pub use risk::BacktestChecks;
pub use types::{EquityPoint, ExchangeId, InstrumentId, Side, StrategyId, Symbol, TradingState};

pub use execution::{PaperConfig, SimGateway, SyncExecutionGateway, SyncPaperGateway};
pub use instrument::{
    IndexedInstruments, InstrumentFilter, InstrumentIndex, InstrumentMeta, InstrumentRegistry,
    SystemConfig,
};
pub use metrics::{
    strategy_position_report, trading_summaries_per_strategy, StrategyPositionRow, TradingSummary,
};
pub use oms::OrderStore;
pub use results::{append_results_jsonl, write_backtest_json, write_backtest_outputs};
pub use risk::{DefaultRiskManager, RiskManager};
pub use state::GlobalState;
pub use strategy::Strategy;
