# Changelog

## Unreleased

### Breaking cleanup

- Consolidated execution on `PaperExecution` and risk checks on `RiskEngine`.
- Removed the disconnected Barter-style configuration/index API, generic batch replay wrappers,
  and the non-replayable `pallas-merge` binary.
- Replaced the vendored C++ JSON header with a protocol-specific standard-library SDK.

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
  `SmallVec<[AccountEvent; 4]>` for the sync paper-gateway fill buffers, converting to `Vec` only
  when building owned report data.
- Added `instrument::ticks` (`PriceTicks`/`QtyLots` i64 newtypes) for exact, allocation-free
  tick/lot arithmetic with Decimal round-trip and notional-equivalence tests; paper `notional()`
  uses the integer path when price and quantity are on-grid.

### Features / correctness

- Removed bar lookahead: orders emitted from a completed OHLCV bar wait for the next market update;
  next-open execution now honors `half_spread_bps`, and unexecuted final-bar orders remain visible.
- Trade ledgers allocate opening and closing fees; reports now include effective parameters, data
  metadata, total fees, turnover, structured rejections, pending/client/OCO order details, final
  positions, and RFC3339 timestamps.
- External protocol v2 sends full multi-instrument metadata and arbitrary strategy parameters,
  reports fills/rejections/working orders, and supports cancel-by-id, OCO groups, and a final
  flatten callback while retaining the legacy single-instrument handshake.
- Built-in buy-and-hold resolves quantity from explicit config or instrument lot semantics instead
  of assuming fractional shares.
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
- CI: removed stale non-workspace jobs and the non-existent `data-fetch` feature check; current CI
  covers fmt, clippy, `athenas-pallas` tests, tools build, Python/C++ strategy golden tests,
  examples, workspace tests, dependency caching via `Swatinem/rust-cache`, and a benchmark
  regression gate comparing `noop_100k_amortized` against a committed ceiling
  (`docs/bench_baseline.json`).
- Refreshed `docs/benchmarks.txt` and the README/PERFORMANCE per-bar claim to be host-qualified.

## 3.1.0

- Derivatives coverage: option/perpetual/bond metadata in the registry and `IndexedInstruments`;
  futures fee notional uses `contract_multiplier`; initial margin via `margin_required`; perp
  funding and bond-coupon lifecycle hooks; European option exercise at expiry.
- Multi-instrument backtests through `[[instruments]]` config plus a streaming k-way source merge;
  canonical OHLCV and FX L1 sources; `pallas-resample` and `pallas-sweep` CLIs.
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
