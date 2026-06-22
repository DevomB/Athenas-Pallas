# Rust Optimization Audit

This is a backlog of performance work that would make the engine stronger technically and easier to discuss in interviews or resume bullets. The engine already has a strong baseline: fixed-width `Bar`, dense instrument indices, `.pbar` sidecars, streaming source merge, and a synchronous replay path. The next improvements should be benchmark-backed.

## Highest Impact

### 1. Store Bar Replay Events as Borrowed Views — DONE

Implemented as `ReplayEvent<'a>` in `events.rs`, `Strategy::on_replay_event`, and
`dispatch_replay_bar_sync`. The tick-replay loop calls `BarSeriesSource::bar_to_replay_event`
instead of materializing `Event::Market(Bar)` each bar.

### 2. Replace Passive Order Polling Scan with Price-Indexed Resting Orders — DONE

`oms/mod.rs` maintains per-instrument `BTreeMap` books (`buy_limits`, `sell_limits`, `buy_stops`,
`sell_stops`) with `SmallVec<[OrderId; 4]>` at each price level. `OrderStore::pollable_ids` returns
only candidates whose limit/stop could cross against the current L1 or bar high/low; paper
`poll_instrument_into` evaluates fill rules on that subset only.

### 3. Cache Period Inference Without Allocating Timestamps — DONE

`BarSeries::infer_periods_per_year` walks bar timestamps in place; `resolve_periods_per_year`
calls it instead of building a `Vec<OffsetDateTime>`.

### 4. Make Equity Curve Optional Mean "No Summary Allocation" — DONE

`RollingMetrics::streaming_summary` produces a full `PerformanceSummary` from O(1) streamed state
when `record_equity_curve = false`; `finalize_report` in `runner.rs` selects curve vs streaming path.

### 5. Avoid Async Engine Snapshot Clones

Current path: `engine.rs`, `execution/mod.rs`

The synchronous backtest path already avoids most snapshots. The async/live engine still clones `GlobalState` before strategy and execution calls. Split gateway traits into read-only borrowed methods where possible:

```rust
async fn place_market_ref(&self, state: &GlobalState, intent: &OrderIntent) -> Result<Vec<AccountEvent>>;
```

For strategies that require immutable snapshots across await points, use `Arc<GlobalStateSnapshot>` with narrowed fields instead of cloning the full state.

Expected impact: major constant-factor reduction for live/paper mode with many instruments, balances, or orders.

Proof: use existing `snapshot_clone` Criterion bench and add async dispatch benchmarks.

## Medium Impact

### 6. Deduplicate Timestamp Extraction — DONE

Current paths: `engine.rs`, `backtest/runner.rs`, `backtest/merge.rs`, `backtest/batch.rs`, `audit.rs`, `system.rs`

Five local `event_ts`/`event_time`/`event_timestamp`/`equity_ts`/`event_ts_unix_ns` helpers with
divergent fallback behavior were consolidated into `events.rs`:

```rust
impl Event {
    pub fn timestamp(&self) -> Option<OffsetDateTime> { ... }
    pub fn timestamp_or_now(&self) -> OffsetDateTime { ... }
    pub fn timestamp_unix_nanos(&self) -> Option<i128> { ... }
}
```

The `Option`-returning form means account/control events no longer trigger `now_utc()` in replay
paths; callers opt into the wall-clock fallback explicitly via `timestamp_or_now`. The k-way merge
keeps its `UNIX_EPOCH` ordering fallback. Behavior is unchanged; the duplication and accidental
wall-clock reads are gone.

Proof: existing replay/merge/audit tests stay green; benchmark with Criterion `session_overhead_100k`.

### 7. Intern or Densify Asset and Instrument IDs

Current paths: `types.rs`, `instrument/registry.rs`, `state.rs`, `execution/paper.rs`

The registry already uses dense instrument indices, but balances and strategy attribution still use string-backed keys. Add dense `AssetIndex` and store balances in `Vec<Decimal>` once instruments are registered.

Expected impact: less hashing and cloning in fills, equity marking, and multi-instrument runs.

Proof: benchmark multi-instrument replay with 100 instruments and frequent fills.

### 8. Replace `HashMap` With Faster Hashers Where DoS Resistance Is Irrelevant

Current paths: `state.rs`, `instrument/registry.rs`, `metrics.rs`

For internal deterministic maps keyed by small trusted IDs, consider `rustc_hash::FxHashMap` or `hashbrown::HashMap` with a faster hasher. Keep standard `HashMap` at public or untrusted boundaries.

Expected impact: small but measurable in state-heavy runs.

Proof: benchmark strategy attribution and registry lookup hot loops.

### 9. Use `SmallVec` for Tiny Event/Intent Buffers

Current paths: `engine.rs`, `execution/paper.rs`

Most execution calls return 0 to 4 account events. Replace short-lived `Vec<AccountEvent>` returns in the hot paper gateway with `SmallVec<[AccountEvent; 4]>`, then convert only at trait boundaries that need `Vec`.

Expected impact: removes heap allocation for common fills/cancels.

Proof: benchmark buy-and-hold and dense fill scenarios.

### 10. Split External Strategy Serialization From In-Process Strategy Events

Current path: `strategy/external.rs`

External strategies require JSON and owned strings. Keep that boundary, but avoid dragging external-friendly owned event structures into in-process paths. If `ReplayEvent<'_>` is added, external strategy stays on `EventMsg` while Rust strategies get borrowed data.

Expected impact: lower overhead for Rust-native strategy development.

Proof: compare Rust strategy benchmark against Python external strategy baseline.

## Lower-Level Rust Work

### 11. Zero-Copy `.pbar` Reads With `bytemuck` or Explicit Checked Casts

Current paths: `backtest/pbar.rs`, `backtest/bar.rs`

`Bar` is `repr(C)` and fixed-width. A checked zero-copy reader can mmap or read raw bytes into aligned `Bar` slices after validating magic/version/length. Do this only with a clear safety comment and tests for corrupt files, endianness, and alignment.

Expected impact: faster cold starts for huge `.pbar` files.

Proof: benchmark reading 1GB `.pbar`; compare standard read vs mmap.

### 12. SIMD / Chunked Decimal-to-Tick Conversion

Current path: `backtest/bar.rs`

CSV load still parses `Decimal` then converts to ticks one field at a time. For high-volume ingestion, parse Databento/DBN fixed-point prices directly into ticks or add a specialized fast path for integer-like CSV fields.

Expected impact: faster ingestion, not replay.

Proof: benchmark CSV load for 10m rows.

### 13. Use `NonZero` and Niche-Friendly Option Layouts

Current paths: `backtest/bar.rs`, `state.rs`

For dense optional arrays like `Vec<Option<...>>`, consider sentinel-based fixed arrays or compact structs when memory pressure matters. Example: replace several parallel `Vec<Option<Decimal>>` bar fields with one `Vec<LastBarState>`.

Expected impact: better cache locality and fewer branches in `mid_or_last_ix`.

Proof: benchmark state update plus equity mark for many instruments.

### 14. Typed Price/Quantity Newtypes Around Ticks

Current paths: `backtest/bar.rs`, `execution/paper.rs`, `state.rs`

Keep `Decimal` for public reporting, but introduce internal `PriceTicks(i64)` and `QtyLots(i64)` where math is mechanical. This makes units explicit and enables integer math through fills and fees.

Expected impact: large constant-factor improvement in fill-heavy backtests.

Proof: benchmark dense order/fill scenarios; validate exact PnL against existing Decimal tests.

### 15. Enum Dispatch for Common Built-In Strategies and Fill Models

Current paths: `strategy/mod.rs`, `backtest/runner.rs`, `execution/paper.rs`

Trait objects are flexible but cost indirect calls. For hot built-ins, offer enum-backed dispatch:

```rust
enum StrategyImpl { Noop(NoopStrategy), BuyAndHold(BuyAndHold), External(ExternalStrategy) }
```

Keep trait support for extensions.

Expected impact: small improvement in ultra-light strategies where dispatch overhead is visible.

Proof: Criterion no-op replay with dynamic vs enum dispatch.

## Documentation/Resume Framing

Strong resume bullets should be tied to measured outcomes:

- "Implemented cache-friendly fixed-point OHLCV replay using contiguous `repr(C)` bars and binary sidecar caches."
- "Reduced multi-instrument replay memory from materialized event vectors to streaming k-way merge with one pending event per source."
- "Designed a benchmark-backed order-trigger index reducing passive fill checks from O(bars * orders) to O(log orders + fills)."
- "Added optional Rust-native Databento ingestion path using DBN fixed-point decoding into engine-compatible OHLCV CSV."

## Verification Plan

1. Add Criterion cases for each optimization before coding it.
2. Record wall time, ns/bar, allocations if available, and peak RSS for large runs.
3. Keep numerical golden tests around fills, PnL, and drawdown unchanged.
4. Promote only improvements that show measurable benefit on realistic workloads.
