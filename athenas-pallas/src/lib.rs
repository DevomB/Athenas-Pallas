//! # Athena's Pallas
//!
//! Unified event-driven engine for **live**, **paper**, and **backtest** trading.
//!
//! ## Modes
//! - Swap the [`execution::ExecutionGateway`] implementation and event sources; keep
//!   [`strategy::Strategy`] and [`risk::RiskCheck`] unchanged.
//!
//! ## Extension
//! - Add live venues: implement [`connectors::MarketConnector`].
//! - Add fill logic: document custom models via [`backtest::FillModel`].
//!
//! ## Features
//! - `data-fetch` - Alpha Vantage daily market data fetcher.
//! - `binance` - public WebSocket connector for Binance Spot.
//! - `control-server` - localhost HTTP control (`/pause`, `/resume`, `/cancel-all`).
//! - `binance-live` - signed Spot REST ([`execution::BinanceLiveGateway`]), user stream connector, signing deps (`hmac`, `sha2`, `hex`).

#![warn(missing_docs)]

pub mod audit;
pub mod backtest;
pub mod calendar;
pub mod connectors;
pub mod data;
pub mod engine;
pub mod error;
pub mod events;
pub mod execution;
pub mod instrument;
pub mod integration;
pub mod metrics;
pub mod oms;
pub mod replica;
pub mod results;
pub mod risk;
pub mod state;
pub mod strategy;
pub mod system;
pub mod types;

#[cfg(feature = "control-server")]
pub mod control;

pub use engine::{
    dispatch_event, dispatch_event_audited, dispatch_event_sync, dispatch_replay_sync,
    dispatch_strategy_sync, engine_step, Engine, EngineBuilder, EngineCommand, EngineConfig,
    EngineHandle, TimerSchedule,
};
pub use error::{Error, Result};
pub use events::{BookL2Snapshot, Event, OrderIntentSource};
pub use risk::BacktestChecks;
pub use types::{EquityPoint, ExchangeId, InstrumentId, Side, StrategyId, Symbol, TradingState};

pub use execution::{PaperConfig, PaperGateway, SimGateway};
pub use instrument::{
    IndexedInstruments, InstrumentFilter, InstrumentIndex, InstrumentMeta, InstrumentRegistry,
    SystemConfig,
};
pub use metrics::{
    strategy_position_report, trading_summaries_per_strategy, StrategyPositionRow, TradingSummary,
};
pub use oms::OrderStore;
pub use replica::EngineStateReplica;
pub use results::{append_results_jsonl, write_backtest_json, write_backtest_outputs};
pub use risk::{DefaultRiskManager, RiskManager};
pub use state::GlobalState;
pub use strategy::Strategy;
pub use system::{
    AuditMode, EngineFeedMode, EngineSummary, LiveClock, SummaryPeriod, System, SystemArgs,
    SystemBuilder, SystemHandle, TradingSummaryGenerator,
};
