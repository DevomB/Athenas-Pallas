# Changelog

## 2.0.0

### Unreleased

- Single-crate backtest path: `pallas-backtest` CLI, sync `dispatch_event_sync`, `BarSeries`, `ExternalStrategy` (Python/C++ JSON IPC).
- Inlined `pallas-instrument` into `athenas-pallas`; removed `pallas-data`, `pallas-execution`, `pallas-macro` from the workspace.
- Test fixtures under `athenas-pallas/tests/fixtures/`; `data/` is a gitignored fetch workspace.
- Python SMA proof strategy in `Backtesting-Engine/trading/` with golden integration test.
- Removed the Tauri desktop app; workflows are CLI/Rust-only.

### Historical (pre-consolidation)

- Split workspace into `pallas-instrument`, `pallas-integration`, `pallas-data`, `pallas-execution`, `pallas-macro`, and `athenas-pallas` (core engine).
- Barter-style `SystemConfig`, `IndexedInstruments`, `SystemBuilder`, audit replica, `EngineFeedMode::Iterator`, and `InstrumentFilter::None`.
- Multi-venue public data connectors and `StreamBuilder` / `init_indexed_multi_exchange_market_stream`.
- `MockExchange` / `ExecutionClient` in `pallas-execution`.
