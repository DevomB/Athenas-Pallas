# Performance Characteristics

This document explains how the backtest hot path is optimized and how to reproduce benchmark numbers.

## Design Goals

- Single-threaded synchronous replay for deterministic backtests.
- Cache-friendly OHLCV storage via fixed-point ticks in a contiguous `Vec<Bar>`.
- Dense instrument indices: positions and L1 state live in `Vec` rows, not per-tick `HashMap` lookups.
- Streaming I/O for CSV load, preview, and multi-source merge paths.
- Preallocated reusable buffers in replay loops.

## `repr(C) Bar` Layout

Each OHLCV bar is stored as i64 ticks in `athenas-pallas/src/backtest/bar.rs`.

| Field | Type | Role |
|-------|------|------|
| `ts_unix_nanos` | `i64` | UTC timestamp |
| `open/high/low/close_ticks` | `i64` | Price scaled by `1 / tick_size` |
| `volume_lots` | `i64` | Volume in lot units |

Default `tick_size = 1e-8`. Decimal conversion happens at CSV load and JSON/report boundaries. Fills, fees, and margin checks still use `Decimal`.

## Hot Path

When `Strategy::uses_tick_replay()` is true and data is OHLCV:

1. `BarSeries::from_csv_path_or_pbar` loads a binary `.pbar` cache or streams CSV through `csv::Reader<BufReader<File>>`.
2. The replay loop calls `apply_bar`, strategy dispatch, risk checks, paper fills, and lifecycle hooks.
3. Intent buffers are reused with `Vec::with_capacity(4)`.

For multi-instrument backtests, `merge_sources_iter` performs a streaming k-way merge over a `BinaryHeap`, holding only one pending event per source instead of materializing the entire merged event stream.

## Binary `.pbar` Cache

First CSV load can write a `.pbar` sidecar through `backtest/pbar.rs`. Subsequent loads read the binary file directly. This uses standard file I/O today; memory mapping should only be added if benchmarks show `.pbar` read time is a real bottleneck.

## Benchmarks

Run Criterion:

```bash
cargo bench -p athenas-pallas --bench backtest_hotpath
```

Baseline in `docs/benchmarks.txt`: sub-microsecond/bar amortized for a noop strategy on 100k bars (roughly 0.43–0.60 us/bar depending on host). Re-run on the target host before quoting absolute numbers; use the committed Criterion baseline for regression comparison.

## Stress Run

```bash
cargo run --release -p athenas-pallas --example stress_backtest -- 1000000
```

Pass bar count as the first argument. For peak RSS, use platform tools such as `/usr/bin/time -v` on Linux.

## Future Work

- i64 tick math through fills and fees.
- Optional mmap-backed `.pbar` reads if benchmark-backed.
- Sorted resting-order index with binary search on bar high/low.
- CI benchmark regression gate.

## Scope

This engine optimizes research throughput and local decision overhead. It intentionally does not claim exchange co-location latency, kernel bypass, or L2 queue-position simulation.
