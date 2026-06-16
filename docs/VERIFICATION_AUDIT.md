# Verification audit — pass 1

**Date:** 2026-06-15  
**Scope:** Master implementation checklist + Extra gaps table (plan `backtest_engine_gap_analysis_4d0a8fe0`)  
**Tests:** `cargo test -p athenas-pallas` — **PASS** (after `option_meta` strike parse fix in `instrument/index.rs`)

This file is the audit log (plan file is read-only). Pass 2 is required before `verify-audit-loop` can be marked complete.

---

## Summary

| Phase | Honest status | Checklist items done (approx.) |
|-------|---------------|--------------------------------|
| Phase 0 — Trust | **Mostly done** | 6 / 7 |
| Phase 0b — Perf | **Partial** | 1 / 6 |
| Phase 1 — Intraday | **Mostly done** | 6 / 7 |
| Phase 2 — Multi/FX | **Partial** | 4 / 6 |
| Phase 3 — Derivatives | **Partial** | 2 / 6 |
| Phase 4 — Debt/hybrid | **Minimal** | 1 / 4 |
| Phase 5 — Polish | **Partial** | 7 / 8 |
| Verification loop | **Not done** | 0 / 4 |

**Previously over-marked todos corrected:** Phases 2–5 and `verify-audit-loop` were marked `completed` in session todos but are **not** complete per plan DoD.

---

## Master checklist (item-by-item)

### Phase 0 — Trust / correctness

| Item | Status | Evidence / gap |
|------|--------|----------------|
| Futures fee notional uses `contract_multiplier` | **DONE** | `execution/paper.rs` `notional()` includes Future/Perpetual/Option with multiplier |
| Single unified limit fill model (place + poll) | **DONE** | Both use `crossing_limit` + shared `FillModel` trait |
| OHLC high/low touch for bar replay fills | **PARTIAL** | Stops use bar high/low; limits still use synthetic L1 bid/ask from close |
| `lot_size` / `tick_size` enforced at submission | **DONE** | `normalize_order` in `paper.rs` |
| Auto `periods_per_year` | **DONE** | `runner.rs` + `interval.rs` |
| FetchPanel + CLI interval presets | **DONE** | `intervals.rs`, `FetchPanel.tsx`, `pallas-fetch --list-intervals` |
| CSV schemas per asset class | **DONE** | `data/README.md` (updated: Adj Close, FX sources, bonds, futures) |

### Phase 0b — Portfolio performance

| Item | Status | Evidence / gap |
|------|--------|----------------|
| i64 tick math through fills/fees | **NOT DONE** | Decimal in execution path |
| mmap `.pbar` | **NOT DONE** | File I/O only in `pbar.rs` |
| Sorted resting-order index | **NOT DONE** | Linear scan in `poll_after_market_sync` |
| `docs/PERFORMANCE.md` | **DONE** | Written; claims corrected (no false CI gate) |
| Stress 10M bars + peak RSS | **NOT DONE** | Example defaults 100k; RSS not measured in-repo |
| Criterion CI regression gate | **NOT DONE** | CI runs bench; no baseline compare / fail |

### Phase 1 — Intraday + stops

| Item | Status | Evidence / gap |
|------|--------|----------------|
| StopMarket / StopLimit | **DONE** | `types.rs`, `paper.rs`, `stop_orders_backtest.rs` |
| OHLC intrabar rules for stops | **DONE** | `stop_triggered` + bar high/low |
| `pallas-resample` CLI | **DONE** | `bin/pallas-resample.rs` |
| Equity RTH + **holidays** | **PARTIAL** | RTH in `calendar/mod.rs`; comment says "no holidays" |
| MaxPositionSize in BacktestChecks | **DONE** | `runner.rs` wires `MaxPositionSize` |
| Position sizer (% equity) in strategy SDK | **NOT DONE** | No sizer helper in Python/Rust SDK |
| Trade ledger + win rate + profit factor | **DONE** | `metrics.rs`, report fields |

### Phase 2 — Multi-instrument + FX

| Item | Status | Evidence / gap |
|------|--------|----------------|
| `pallas-merge` CLI | **DONE** | `bin/pallas-merge.rs` |
| Multi-instrument TOML `[[instruments]]` | **DONE** | `session.rs`, `runner.rs`, `backtest.toml.example` |
| FX CSV template + free sources | **DONE** | `data/README.md`, `FxCsvSource`, `fx_l1_backtest.rs` |
| Forex 24/5 + pip/lot defaults | **DONE** | `calendar/mod.rs`, forex meta in `config.rs` |
| Per-strategy PnL attribution | **NOT DONE** | State tracks `strategy_id`; report has no per-strategy PnL section |
| Hybrids multi-leg example + test | **NOT DONE** | No hybrid example or test |

### Phase 3 — Derivatives

| Item | Status | Evidence / gap |
|------|--------|----------------|
| Option + Perpetual in registry | **DONE** | `registry.rs`, `config.rs`, `index.rs` (fixed strike parse) |
| Margin engine (initial/maintenance, liquidation) | **PARTIAL** | Initial margin only (`margin_required`); no maintenance/liquidation |
| Futures roll / continuous contract | **NOT DONE** | No roll tooling |
| Perp funding rate schedule | **PARTIAL** | Funding every bar with fixed 8h rate, not scheduled |
| European options exercise at expiry | **PARTIAL** | `lifecycle.rs` hook; strike from meta `face_value`; limited tests |
| Futures CSV convention documented | **DONE** | `data/README.md` Futures section |

### Phase 4 — Debt + hybrids

| Item | Status | Evidence / gap |
|------|--------|----------------|
| Bond with coupon schedule | **PARTIAL** | `lifecycle.rs` + unit test in `pricing.rs`; no integration backtest |
| Bond CSV + worked example + test | **NOT DONE** | Schema in README only |
| Yield/duration reporting | **NOT DONE** | |
| Hybrid convertible example | **NOT DONE** | |

### Phase 5 — Polish + extra gaps

| Item | Status | Evidence / gap |
|------|--------|----------------|
| `pallas-sweep` CLI | **DONE** | `bin/pallas-sweep.rs` |
| GUI equity curve >50k bars | **DONE** | `commands.rs` records + downsample flag |
| Protocol strategy_id / client order id | **DONE** | `strategy/protocol.rs`, `trading/protocol.md` |
| Sharpe/Sortino minus risk-free | **DONE** | `summarize_with_fills_and_rf` |
| Yahoo Adj Close | **DONE** | `yahoo.rs` `effective_close()` |
| IndexedInstruments meta for option/perp | **DONE** | `index.rs` (this pass) |
| SQLite/JSON result persistence | **NOT DONE** | DSQL schema only; runner writes JSON report |
| Extra gaps table closed | **NOT DONE** | See below |

---

## Extra gaps table (plan lines 85–103)

| Gap | Resolution |
|-----|------------|
| Futures fee wrong notional | **fixed** |
| Two limit-fill rules | **fixed** (unified model) |
| lot/tick ignored | **fixed** |
| L2 book unused for fills | **wontfix** (documented; spread model only) |
| IndexedInstruments spot-only meta | **fixed** (this pass) |
| Backtest subset of risk checks | **partial** — MaxPosition + MaxDailyLoss wired; not full `RiskEngine` |
| Protocol drops strategy_id / client id | **fixed** |
| Sharpe ignores risk-free | **fixed** |
| GUI skips equity >50k | **fixed** |
| DSQL never written | **not done** |
| Yahoo Adj Close ignored | **fixed** |
| No corporate-action table | **wontfix** (deferred) |
| batch.rs no CLI | **fixed** (`pallas-sweep`) |
| JSONL replay undocumented | **partial** — not in main README |
| LiveGateway stub | **wontfix** (Binance live only) |
| Close-only synthetic L1 | **partial** — stops use OHLC; limits use touch |
| Python subprocess live latency | **wontfix** (by design for research) |

---

## Actions taken this pass

1. Fixed compile: `InstrumentKind::Option` strike `String` → `Decimal` in `index.rs`
2. Fixed option TOML meta: strike vs tick increment in `config.rs`
3. Corrected `docs/PERFORMANCE.md` (removed false CI regression claim)
4. Updated `data/README.md` (Adj Close, FX, bonds)
5. Added `[[instruments]]` comment to `backtest.toml.example`
6. Re-ran full test suite — pass
7. Reset session todos to honest statuses

---

## Open work (priority)

1. Phase 0b: CI bench gate or document as deferred; 10M stress + RSS measurement
2. Phase 1: equity holiday calendar; position sizer in SDK
3. Phase 2: per-strategy PnL in report; hybrid example
4. Phase 3: futures roll; maintenance margin + liquidation; scheduled funding
5. Phase 4: bond integration test + yield/duration
6. Phase 5: result persistence (SQLite local)
7. Pass 2 audit after fixes; zero findings required for `verify-audit-loop`

---

## Smokes not run this pass

End-to-end smokes per asset class (plan step 5) were **not** executed in this pass:

- crypto, equity — covered by existing integration tests
- forex CSV — `fx_l1_backtest.rs` exists but not re-run manually
- futures, bond, hybrid — **no dedicated smoke tests**

Recommend pass 2 include explicit smoke commands logged here.
