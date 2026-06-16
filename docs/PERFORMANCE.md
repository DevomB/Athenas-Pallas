# Performance characteristics (Athena's Pallas)

This document explains how the backtest hot path is optimized and how to reproduce benchmark numbers.

## Design goals

- **Single-threaded sync replay** for deterministic backtests (no async overhead in the inner loop).
- **Cache-friendly bar storage** via fixed-point ticks in a contiguous `Vec<Bar>`.
- **Dense instrument indices** — positions and L1 live in `Vec` rows, not `HashMap` per tick.
- **Pre-allocation** — `BarSeries` loads once; intent buffer reused with `Vec::with_capacity(4)`.

## `repr(C) Bar` layout

Each OHLCV bar is stored as i64 ticks (see `athenas-pallas/src/backtest/bar.rs`):

| Field | Type | Role |
|-------|------|------|
| `ts_unix_nanos` | i64 | UTC timestamp |
| `open/high/low/close_ticks` | i64 | Price × (1/tick_size) |
| `volume_lots` | i64 | Volume in lot units |

Default `tick_size = 1e-8`. Decimal conversion happens at CSV load and JSON report boundaries. **Fills, fees, and margin checks still use `Decimal` today** (Phase 0b tick-native fills not yet implemented).

## Hot path (tick replay)

When `Strategy::uses_tick_replay()` is true and data is OHLCV:

1. `BarSeries::from_csv_path_or_pbar` — one allocation for all bars (optional `.pbar` sidecar written on first CSV load).
2. Loop: `apply_bar` → `dispatch_replay_sync` (strategy → risk → paper fills).
3. No per-bar `Event` allocation on the inner OHLCV path.

## Binary `.pbar` cache

First CSV load can write a `.pbar` sidecar (`backtest/pbar.rs`). Subsequent loads read the binary file directly. **Not memory-mapped yet** — uses standard file I/O.

## Benchmarks

Run Criterion suite:

```bash
cargo bench -p athenas-pallas --bench backtest_hotpath
```

Baseline (see `docs/benchmarks.txt`): ~0.43 µs/bar amortized for noop strategy on 100k bars.

CI runs `bench-regression` job which **executes** the bench target but does **not** yet compare against baseline or fail on >5% regression (planned Phase 0b item).

## Stress run

```bash
cargo run --release -p athenas-pallas --example stress_backtest -- 1000000
```

Pass bar count as first argument (default 100_000). Wall time is printed; for peak RSS use platform tools (e.g. `/usr/bin/time -v` on Linux).

## Not yet implemented (Phase 0b)

- i64 tick math through fills/fees
- mmap `.pbar` reads
- Sorted resting-order index + binary search on bar high/low
- Criterion CI regression gate (>5% slowdown fails)

## What we intentionally skip

- L2 queue simulation (spread model only).
- Multi-threaded replay of a single strategy (parallelism is for parameter sweeps via `pallas-sweep`).
- Kernel bypass / co-location tooling.

## Laptop vs co-lo HFT

Retail API round-trip latency dominates live trading from a laptop. This engine optimizes **research throughput** and **local decision overhead**, not exchange co-location.
