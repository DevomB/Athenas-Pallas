# Verification audit

## Current installed surface check (2026-07-19)

This repo is currently CLI/Rust-only. Installed workspace crates are `athenas-pallas` and
`athenas-pallas-tools`; binaries are `pallas-backtest`, `pallas-resample`, and `pallas-sweep`.
`athenas-pallas` currently exposes `default`, `databento`, and `tracing-full`
features.

Market data replay is local-file based: CSV/pbar via `pallas-backtest` and CSV utilities in
`athenas-pallas-tools`. The optional `databento` feature adds a Databento OHLCV cache/export path
that writes engine CSVs under `data/databento` before replay. There is no installed Alpha Vantage,
Binance-live, or generic fetch crate/feature/binary in the current manifests or lockfile.

## Pass 3 update (2026-06-22)

Quality and docs sweep. `cargo fmt --all --check`, `cargo test -p athenas-pallas`,
`cargo test -p athenas-pallas --all-features`, `cargo test -p athenas-pallas-tools`, and
`cargo clippy -p athenas-pallas --all-targets --all-features -- -D warnings` all **PASS**.

Updates since pass 2:

| Area | Resolution |
|------|------------|
| Order-trigger candidate allocation | **done** - `OrderStore::pollable_ids_into` lets the fill engine reuse a `SmallVec<[OrderId; 16]>` candidate buffer while preserving the public `pollable_ids -> Vec<OrderId>` API. |
| Fill event allocation | **done** - fill emission now appends order, fill, and balance events directly into `AccountEvents` (`SmallVec<[AccountEvent; 4]>`) without a temporary balance-update `Vec`. |
| Per-strategy PnL grouping | **done** - `metrics::per_strategy_pnl` groups borrowed fill records instead of cloning `FillRecord`s per strategy bucket. |
| Quote equity marking | **done** - single-quote equity no longer allocates a `HashSet`; aggregate quote collection uses the existing trusted fast-hash collection path. |
| Current docs surface | **done** - README, performance notes, and optimization audit now describe the optional Databento exporter and the current replay-core contract. |

---

## Pass 2 update (2026-06-21)

Optimization + correctness sweep. `cargo test -p athenas-pallas` and
`cargo clippy -p athenas-pallas --all-targets -- -D warnings` both **PASS**.

Gaps closed since pass 1:

| Pass-1 gap | Pass-2 resolution |
|------------|-------------------|
| Phase 0b: i64 tick math through fills/fees | **partial** - `instrument::ticks` (`PriceTicks`/`QtyLots`) with tests; paper `notional()` uses the integer fast path when price/qty are on-grid. Full Decimal pipeline retained for off-grid and reporting. |
| Phase 0b: sorted/indexed resting orders | **done** - per-instrument `BTreeMap` price indices (`buy_limits`, `sell_limits`, `buy_stops`, `sell_stops`) with `OrderStore::pollable_ids` so passive fill checks visit only price levels touched by L1/bar high-low (`O(log m + k)`). |
| Phase 0b: Criterion CI regression gate | **done** - CI compares `noop_100k_amortized` mean against a committed ceiling in `docs/bench_baseline.json`. |
| Phase 1: equity holidays | **done** - NYSE fixed/floating holidays + observance + Good Friday in `calendar/mod.rs`, wired into `EquityRth`. |
| Phase 1: position sizer (% equity) | **done** - `strategy::sizing::position_size_pct_equity` + `StrategyContext::size_pct_equity`, mirrored in Python/C++ SDKs. |
| Phase 2: per-strategy PnL in report | **done** - `FillRecord.strategy_id` + `metrics::per_strategy_pnl` -> `BacktestReport.per_strategy`. |
| Phase 3: maintenance margin + liquidation | **done** - `maintenance_margin_required` + mid-price liquidation of underwater leveraged derivatives in `lifecycle.rs`. |
| Phase 3: scheduled perp funding | **done** - funding settles only on 00:00/08:00/16:00 UTC boundaries, not every bar. |
| Hot loop per-bar allocation | **done** - `ReplayEvent<'a>` + `on_replay_event` + `dispatch_replay_bar_sync` remove the per-bar `Event` alloc and `InstrumentId` clone. |
| Streaming metrics / startup Vec alloc | **done** - O(1) `RollingMetrics` summary when the curve is off; `BarSeries::infer_periods_per_year`. |

Still open (deferred, not regressions): mmap `.pbar`; 10M-bar stress + peak RSS;
futures roll / continuous contracts; bond integration backtest + yield/duration;
hybrid convertible example; SQLite result persistence; pass-3 zero-finding audit.

---

# Verification audit - pass 1

**Date:** 2026-06-15  
**Scope:** Master implementation checklist + Extra gaps table (plan `backtest_engine_gap_analysis_4d0a8fe0`)  
**Tests:** `cargo test -p athenas-pallas` - **PASS** (after `option_meta` strike parse fix in `instrument/index.rs`)

This section is retained as historical audit context. The current installed-surface and pass-2
sections above supersede stale pass-1 status rows where they conflict.

---

## Summary

| Phase | Honest status | Checklist items done (approx.) |
|-------|---------------|--------------------------------|
| Phase 0 - Trust | **Mostly done** | 6 / 7 |
| Phase 0b - Perf | **Partial** | 1 / 6 |
| Phase 1 - Intraday | **Mostly done** | 6 / 7 |
| Phase 2 - Multi/FX | **Partial** | 4 / 6 |
| Phase 3 - Derivatives | **Partial** | 2 / 6 |
| Phase 4 - Debt/hybrid | **Minimal** | 1 / 4 |
| Phase 5 - Polish | **Partial** | 7 / 8 |
| Verification loop | **Not done** | 0 / 4 |

**Previously over-marked todos corrected:** Phases 2-5 and `verify-audit-loop` were marked `completed` in session todos but are **not** complete per plan DoD.

---

## Master checklist (item-by-item)

### Phase 0 - Trust / correctness

| Item | Status | Evidence / gap |
|------|--------|----------------|
| Futures fee notional uses `contract_multiplier` | **DONE** | Current implementation is in `execution/fills.rs` `notional()` and includes Future/Perpetual/Option with multiplier |
| Single unified limit fill model (place + poll) | **DONE** | Both use `crossing_limit` + shared `FillModel` trait |
| OHLC high/low touch for bar replay fills | **PARTIAL** | Stops use bar high/low; limits still use synthetic L1 bid/ask from close |
| `lot_size` / `tick_size` enforced at submission | **DONE** | `normalize_order` in `execution/fills.rs` |
| Auto `periods_per_year` | **DONE** | `runner.rs` + `interval.rs` |
| Built-in data downloader | **REMOVED** | Historical downloader code and CLI are no longer part of the crate |
| CSV schemas per asset class | **DONE** | `data/README.md` (updated: Adj Close, FX sources, bonds, futures) |

### Phase 0b - Portfolio performance

| Item | Status | Evidence / gap |
|------|--------|----------------|
| i64 tick math through fills/fees | **PARTIAL** | `instrument::ticks` fast path is used for on-grid notional; Decimal remains the public/off-grid/reporting path |
| mmap `.pbar` | **NOT DONE** | File I/O only in `pbar.rs` |
| Sorted resting-order index | **DONE** | Per-instrument `BTreeMap` price levels plus small candidate buffers in `oms`/`execution::fills` |
| `docs/PERFORMANCE.md` | **DONE** | Written and updated with the current hot-path buffers |
| Stress 10M bars + peak RSS | **NOT DONE** | Example defaults 100k; RSS not measured in-repo |
| Criterion CI regression gate | **DONE** | Baseline file exists in `docs/bench_baseline.json` |

### Phase 1 - Intraday + stops

| Item | Status | Evidence / gap |
|------|--------|----------------|
| StopMarket / StopLimit | **DONE** | `events.rs`, `execution/fills.rs`, `stop_orders_backtest.rs` |
| OHLC intrabar rules for stops | **DONE** | `stop_triggered` + bar high/low |
| `pallas-resample` CLI | **DONE** | `bin/pallas-resample.rs` |
| Equity RTH + **holidays** | **PARTIAL** | RTH in `calendar/mod.rs`; comment says "no holidays" |
| MaxPositionSize in RiskEngine | **DONE** | `runner.rs` wires `MaxPositionSize` |
| Position sizer (% equity) in strategy SDK | **NOT DONE** | No sizer helper in Python/Rust SDK |
| Trade ledger + win rate + profit factor | **DONE** | `metrics.rs`, report fields |

### Phase 2 - Multi-instrument + FX

| Item | Status | Evidence / gap |
|------|--------|----------------|
| `pallas-merge` CLI | **removed** | Output was not replayable by the engine; multi-source runs stream internally. |
| Multi-instrument TOML `[[instruments]]` | **DONE** | `session.rs`, `runner.rs`, `backtest.toml.example` |
| FX CSV template + free sources | **DONE** | `data/README.md`, `FxCsvSource`, `fx_l1_backtest.rs` |
| Forex 24/5 + pip/lot defaults | **DONE** | `calendar/mod.rs`, forex meta in `config.rs` |
| Per-strategy PnL attribution | **DONE** | `BacktestReport.per_strategy` is built from tagged fills |
| Hybrids multi-leg example + test | **NOT DONE** | No hybrid example or test |

### Phase 3 - Derivatives

| Item | Status | Evidence / gap |
|------|--------|----------------|
| Option + Perpetual in registry | **DONE** | `registry.rs`, `config.rs`, `index.rs` (fixed strike parse) |
| Margin engine (initial/maintenance, liquidation) | **PARTIAL** | Initial/maintenance helpers and liquidation hooks exist; broader derivative smoke coverage is still open |
| Futures roll / continuous contract | **NOT DONE** | No roll tooling |
| Perp funding rate schedule | **DONE** | Funding settles on 00:00/08:00/16:00 UTC boundaries |
| European options exercise at expiry | **PARTIAL** | `lifecycle.rs` hook; strike from meta `face_value`; limited tests |
| Futures CSV convention documented | **DONE** | `data/README.md` Futures section |

### Phase 4 - Debt + hybrids

| Item | Status | Evidence / gap |
|------|--------|----------------|
| Bond with coupon schedule | **PARTIAL** | `lifecycle.rs` + unit test in `pricing.rs`; no integration backtest |
| Bond CSV + worked example + test | **NOT DONE** | Schema in README only |
| Yield/duration reporting | **NOT DONE** | |
| Hybrid convertible example | **NOT DONE** | |

### Phase 5 - Polish + extra gaps

| Item | Status | Evidence / gap |
|------|--------|----------------|
| `pallas-sweep` CLI | **DONE** | `bin/pallas-sweep.rs` |
| Desktop equity curve >50k bars | **REMOVED** | Desktop app removed; CLI reports keep full engine output |
| Protocol strategy_id / client order id | **DONE** | `strategy/protocol.rs`, `trading/protocol.md` |
| Sharpe/Sortino minus risk-free | **DONE** | `summarize_with_fills_and_rf` |
| Yahoo Adj Close | **DONE** | `yahoo.rs` `effective_close()` |
| Disconnected IndexedInstruments API | **removed** | Runtime registry is the single dense instrument index. |
| SQLite/JSON result persistence | **NOT DONE** | DSQL schema only; runner writes JSON report |
| Extra gaps table closed | **NOT DONE** | See below |

---

## Extra gaps table (plan lines 85-103)

| Gap | Resolution |
|-----|------------|
| Futures fee wrong notional | **fixed** |
| Two limit-fill rules | **fixed** (unified model) |
| lot/tick ignored | **fixed** |
| L2 book unused for fills | **wontfix** (documented; spread model only) |
| Duplicate instrument indexing | **removed** | Runtime registry is canonical. |
| Backtest subset of risk checks | **partial** - MaxPosition + MaxDailyLoss wired; not full `RiskEngine` |
| Protocol drops strategy_id / client id | **fixed** |
| Sharpe ignores risk-free | **fixed** |
| Desktop skips equity >50k | **removed with desktop app** |
| DSQL never written | **not done** |
| Yahoo Adj Close ignored | **fixed** |
| No corporate-action table | **wontfix** (deferred) |
| batch.rs no CLI | **fixed** (`pallas-sweep`) |
| JSONL replay undocumented | **partial** - not in main README |
| LiveGateway stub | **removed/currently absent** (CLI backtest workflow only) |
| Close-only synthetic L1 | **partial** - stops use OHLC; limits use touch |
| Python subprocess live latency | **wontfix** (by design for research) |

---

## Actions taken this pass

1. Fixed compile: `InstrumentKind::Option` strike `String` -> `Decimal` in `index.rs`
2. Fixed option TOML meta: strike vs tick increment in `config.rs`
3. Corrected `docs/PERFORMANCE.md` (removed false CI regression claim)
4. Updated `data/README.md` (Adj Close, FX, bonds)
5. Added `[[instruments]]` comment to `backtest.toml.example`
6. Re-ran full test suite - pass
7. Reset session todos to honest statuses

---

## Open work (priority)

1. 10M-bar stress + peak RSS measurement
2. Futures roll / continuous-contract tooling
3. Bond integration backtest + yield/duration reporting
4. Hybrid convertible example/test
5. Result persistence beyond JSON/JSONL outputs
6. Pass-3 zero-finding audit

---

## Smokes not run this pass

End-to-end smokes per asset class (plan step 5) were **not** executed in this pass:

- crypto, equity - covered by existing integration tests
- forex CSV - `fx_l1_backtest.rs` exists but not re-run manually
- futures, bond, hybrid - **no dedicated smoke tests**

Recommend pass 2 include explicit smoke commands logged here.
