# Rust Optimization Audit

This is a backlog of performance work that would make the engine stronger technically and easier to discuss in interviews or resume bullets. The engine already has a strong baseline: fixed-width `Bar`, dense instrument indices, `.pbar` sidecars, streaming source merge, and a synchronous replay path. The next improvements should be benchmark-backed.

## Highest Impact

### 1. Store Bar Replay Events as Borrowed Views

Current path: `backtest/bar.rs`, `backtest/runner.rs`, `engine.rs`

The optimized OHLCV path still builds a full `Event::Market(MarketEvent::Bar)` for strategy dispatch. For built-in Rust strategies, add a borrowed replay event such as:

```rust
enum ReplayEvent<'a> {
    Bar { instrument_ix: usize, bar: &'a Bar, ts: OffsetDateTime },
    Event(&'a Event),
}
```

Then add a `Strategy::on_replay_event` fast path with a default adapter. External strategies can keep the JSON event path.

Expected impact: lower allocation/string/id cloning pressure in the hottest loop. Complexity stays O(n), but constant factors improve.

Proof: compare Criterion `noop_100k_amortized` and `buy_and_hold_100k` before/after.

### 2. Replace Passive Order Polling Scan with Price-Indexed Resting Orders

Current path: `execution/paper.rs`

`poll_after_market_sync` scans every open order after each market event. That is O(bars * open_orders). For many resting limits/stops, replace the scan with per-instrument order books:

- buy limits keyed by descending limit price
- sell limits keyed by ascending limit price
- buy stops keyed by ascending stop price
- sell stops keyed by descending stop price

Use `BTreeMap<PriceKey, SmallVec<[OrderId; 4]>>` or a custom sorted `Vec` if order count is usually small. On each bar, only visit price levels touched by high/low or bid/ask.

Expected impact: turns repeated full scans into O(log m + k) where `k` is triggered orders.

Proof: add a benchmark with 10k bars and 1k resting orders where only a small fraction fills.

### 3. Cache Period Inference Without Allocating Timestamps

Current path: `backtest/runner.rs`

`resolve_periods_per_year` builds a `Vec<OffsetDateTime>` from `BarSeries`, then calls `infer_periods_per_year_from_timestamps`. Add `BarSeries::infer_periods_per_year(asset_class)` that walks bars directly and tracks deltas without allocating.

Expected impact: removes an O(n) allocation before each run. Complexity remains O(n), but startup memory drops.

Proof: benchmark backtest startup on 1m+ bar CSV/pbar with `auto_periods_per_year = true`.

### 4. Make Equity Curve Optional Mean "No Summary Allocation"

Current path: `backtest/runner.rs`, `metrics.rs`

When `record_equity_curve = false`, the runner still passes an empty curve into summary logic and loses some metrics. Add a streaming `RunMetrics` that records:

- first equity
- last equity
- Welford return stats
- downside Welford stats
- max drawdown

Then summary can be produced without storing `Vec<EquityPoint>` unless the user explicitly wants the curve.

Expected impact: O(1) memory for large sweeps that do not need chart data.

Proof: benchmark 1m bars with `record_equity_curve` true vs false and report peak RSS.

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

### 6. Deduplicate Timestamp Extraction

Current paths: `engine.rs`, `backtest/runner.rs`, `backtest/merge.rs`, `backtest/batch.rs`, `audit.rs`

There are several local `event_ts`/`event_time` helpers with different fallback behavior. Move this into `events.rs`:

```rust
impl Event {
    pub fn timestamp(&self) -> Option<OffsetDateTime> { ... }
}
```

Use `Option` so non-timestamp account/control events do not call `now_utc()` in replay paths.

Expected impact: small hot-path cleanup and fewer accidental wall-clock reads.

Proof: targeted unit tests plus Criterion `session_overhead_100k`.

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
