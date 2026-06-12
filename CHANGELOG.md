# Changelog

## 2.0.0 (unreleased)

- Single-crate backtest path: `pallas-backtest` CLI, sync `dispatch_event_sync`, `BarSeries`, `ExternalStrategy` (Python/C++ JSON IPC).
- Inlined `pallas-instrument` into `athenas-pallas`; removed `pallas-data`, `pallas-execution`, `pallas-macro` from the workspace.
- Sample data under `data/` (BTCUSDT OHLCV, Yahoo AAPL, FX EURUSD, JSONL events).
- Python SMA proof strategy in sibling `trading/` with golden integration test.

## 2.0.0 (historical)

- Split workspace into `pallas-instrument`, `pallas-integration`, `pallas-data`, `pallas-execution`, `pallas-macro`, and `athenas-pallas` (core engine).
- Barter-style `SystemConfig`, `IndexedInstruments`, `SystemBuilder`, audit replica, `EngineFeedMode::Iterator`, and `InstrumentFilter::None`.
- Multi-venue public data connectors and `StreamBuilder` / `init_indexed_multi_exchange_market_stream`.
- `MockExchange` / `ExecutionClient` in `pallas-execution`.
