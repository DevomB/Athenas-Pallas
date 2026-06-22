# Changelog

## Unreleased

### Performance

- Borrowed replay events: `ReplayEvent<'a>` + `Strategy::on_replay_event` remove the per-bar
  `Event::Market(Bar)` allocation and `InstrumentId` clone from the hottest tick-replay loop, via
  the new `dispatch_replay_bar_sync`.
- Instrument-scoped passive fills: `OrderStore` keeps per-instrument **price-indexed** books
  (`BTreeMap` by limit/stop price per side) and `pollable_ids` so the paper gateway visits only
  orders whose cross/trigger could be satisfied by the current L1 or bar high/low
  (`O(log m + k)`), not every resting order on the instrument.
- Streaming run metrics: with `record_equity_curve = false` the summary is computed from O(1)
  `RollingMetrics` (Welford + drawdown) instead of materializing a `Vec<EquityPoint>`.
- `BarSeries::infer_periods_per_year` derives annualization from median bar spacing without
  allocating a full `Vec<OffsetDateTime>` at startup.
- `FxHashMap` (rustc-hash) for the trusted internal registry `by_id` and `strategy_positions` maps;
  `SmallVec<[AccountEvent; 4]>` for the sync paper-gateway fill buffers (converted to `Vec` only at
  the async boundary).
- Added `instrument::ticks` (`PriceTicks`/`QtyLots` i64 newtypes) for exact, allocation-free
  tick/lot arithmetic with Decimal round-trip and notional-equivalence tests; paper `notional()`
  uses the integer path when price and quantity are on-grid.

### Features / correctness

- Per-strategy realized PnL section in the backtest report (`StrategyPnlRow`), attributed via the
  new `FillRecord.strategy_id`.
- NYSE-style equity holiday calendar wired into `SessionFilter::EquityRth` (fixed + floating
  holidays with weekend-observance rules and Good Friday via Computus).
- Maintenance margin + liquidation: leveraged futures/perps are force-closed at mid when
  mark-to-market equity falls below `maintenance_margin_required`; perpetual funding now settles on
  the standard 00:00/08:00/16:00 UTC schedule instead of every bar.
- `% equity` position sizer helper across the Rust (`strategy::sizing`), Python, and C++ SDKs.

### Infra / cleanup

- Consolidated five duplicated event-timestamp helpers into `Event::timestamp()`,
  `Event::timestamp_or_now()`, and `Event::timestamp_unix_nanos()`. Replay paths now use the
  `Option`-returning form so account/control events no longer trigger accidental wall-clock
  (`now_utc`) reads in the hot loop.
- Cleared the last compiler deprecation (`time::format_description::parse` -> `parse_borrowed`) and
  made `cargo clippy --all-targets -- -D warnings` pass.
- CI: removed the stale Tauri `pallas-app` job, fixed the non-existent `data-fetch` feature (now a
  `binance-live`/`control-server`/`databento`/`all` build+test matrix), added dependency caching via
  `Swatinem/rust-cache`, and added a benchmark regression gate comparing `noop_100k_amortized`
  against a committed ceiling (`docs/bench_baseline.json`).
- Refreshed `docs/benchmarks.txt` and the README/PERFORMANCE per-bar claim to be host-qualified.

## 3.1.0

- Derivatives coverage: option/perpetual/bond metadata in the registry and `IndexedInstruments`;
  futures fee notional uses `contract_multiplier`; initial margin via `margin_required`; perp
  funding and bond-coupon lifecycle hooks; European option exercise at expiry.
- Multi-instrument backtests through `[[instruments]]` config plus a streaming k-way source merge;
  FX L1 and Yahoo (`Adj Close`) data sources; `pallas-merge`, `pallas-resample`, and `pallas-sweep`
  CLIs.
- Reporting: risk-free-adjusted Sharpe/Sortino, trade ledger (win rate, profit factor, closed
  round-trips), and automatic periods-per-year inference from bar spacing.

## 3.0.0

- Single-crate backtest path: `pallas-backtest` CLI, sync `dispatch_event_sync`, `BarSeries`,
  `ExternalStrategy` (Python/C++ newline-JSON IPC).
- Inlined `pallas-instrument` into `athenas-pallas`; removed `pallas-data`, `pallas-execution`, and
  `pallas-macro` from the workspace.
- Test fixtures under `athenas-pallas/tests/fixtures/`; `data/` is a gitignored local data
  workspace.
- Python SMA proof strategy in `trading/` with a golden integration test.
- Removed the Tauri desktop app; workflows are CLI/Rust-only.

## 2.0.0

- Split workspace into `pallas-instrument`, `pallas-integration`, `pallas-data`,
  `pallas-execution`, `pallas-macro`, and `athenas-pallas` (core engine).
- Barter-style `SystemConfig`, `IndexedInstruments`, `SystemBuilder`, audit replica,
  `EngineFeedMode::Iterator`, and `InstrumentFilter::None`.
- Multi-venue public data connectors and `StreamBuilder` /
  `init_indexed_multi_exchange_market_stream`.
- `MockExchange` / `ExecutionClient` in `pallas-execution`.
