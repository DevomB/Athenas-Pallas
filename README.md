# Athena's Pallas

[![CI](https://github.com/DevomB/Athenas-Pallas/actions/workflows/ci.yml/badge.svg)](https://github.com/DevomB/Athenas-Pallas/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/athenas-pallas.svg)](https://crates.io/crates/athenas-pallas)
[![docs.rs](https://img.shields.io/docsrs/athenas-pallas)](https://docs.rs/athenas-pallas)
[![license](https://img.shields.io/crates/l/athenas-pallas.svg)](#license)

Event-driven algorithmic **backtesting** in Rust. Replay CSV or pbar history, run in-process or external C++/Python strategies, and export performance reports.

- Sync CSV replay hot path (sub-microsecond/bar amortized for a noop strategy; ~0.43–0.60 us/bar depending on host — see [benchmarks](docs/benchmarks.txt))
- Python and C++ strategies over newline JSON ([protocol](trading/protocol.md))
- Local CSV, pbar, FX L1, and futures bar workflows

Current installed surface:

- Workspace crates: `athenas-pallas` and `athenas-pallas-tools`
- Rust binaries: `pallas-backtest`, `pallas-resample`, and `pallas-sweep`
- Cargo features on `athenas-pallas`: `default`, `databento`, and `tracing-full`
- Market data ingestion: local CSV/pbar files by default, plus an optional Databento OHLCV cache/export path behind `--features databento`. There is no installed Alpha Vantage, Binance-live, or generic fetch package in this checkout.

## Install

**Requirements:** Rust 1.85+, Python 3 for Python strategies, CMake/g++ for C++ strategies.

```bash
git clone https://github.com/DevomB/Athenas-Pallas.git
cd Athenas-Pallas   # or Backtesting-Engine if that is your repo folder name
cargo build --release -p athenas-pallas
```

Run the test suite:

```bash
cargo test -p athenas-pallas
```

## Quick Demo

Built-in buy-and-hold over the committed EXAMPLE fixture:

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- --data athenas-pallas/tests/fixtures/data/EXAMPLE_1d.csv --instrument test:EXAMPLE --initial-balance USD:10000
```

The built-in strategy uses one configured instrument lot by default (one share for ordinary
equities). Override it explicitly with `--buy-and-hold-qty QTY`.

Direct strategy-name resolution:

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- --data athenas-pallas/tests/fixtures/data/EXAMPLE_1d.csv --instrument test:EXAMPLE --initial-balance USD:10000 --strategy simple_sma
```

External strategies receive arbitrary initialization parameters via repeated
`--param KEY=JSON` arguments or the `[strategy_parameters]` TOML table. JSON reports include the
effective parameters, source/time-range metadata, RFC3339 timestamps, fees, turnover, rejection
counts/details, pending orders and client ids, and final positions.

Or use the helper scripts:

```bash
./scripts/run_sma_backtest.sh    # Unix
./scripts/run_sma_backtest.ps1   # Windows
```

## Strategy Layout

Strategies live directly under `trading/<strategy_name>/`. The engine detects the runtime from the files inside the folder:

```text
trading/
  _sdk/
    python/pallas_strategy.py
    cpp/pallas_strategy.hpp
  simple_sma/
    strategy.py
  simple_sma_cpp/
    CMakeLists.txt
    main.cpp
  pfe_fisher_cpp/
    CMakeLists.txt
    main.cpp
    backtest.toml
```

Detection order:

1. Directory with `CMakeLists.txt`: build with CMake and run the compiled binary.
2. Directory with `strategy.py` or `main.py`: run with the configured Python executable.
3. `.py` file: run with Python.
4. Other file path: run as a binary.

Python children receive a platform-correct `PYTHONPATH` containing the strategy directory plus any
existing `_common/python` and `_sdk/python` directories beside the strategy tree. A catalog strategy
can therefore run directly from its source checkout:

```powershell
cargo run --release -p athenas-pallas --bin pallas-backtest -- `
  --data data\ES.csv --instrument cme:ES `
  --strategy "..\trading-algos\opening_range_rth"
```

Running a source strategy directly avoids copying shared `_common` code. If a strategy is deployed
by copying it into `trading/`, copy its matching `_common/python` directory too; the engine discovers
both layouts automatically and preserves any user-supplied `PYTHONPATH` entries.

## Market Data

Put local CSV exports into `data/` (gitignored local workspace), then point the backtest at the file:

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- --data data/AAPL_live.csv --data-format ohlcv --instrument csv:AAPL --asset-class equity --initial-balance USD:10000
```

Copy [`backtest.toml.example`](backtest.toml.example) to `backtest.toml` and point `[backtest].data` at your file.

The core backtest path remains provider-neutral: `pallas-backtest` replays documented local CSV/pbar files. With the optional `databento` feature, the CLI can fetch Databento OHLCV data into `data/databento/*.csv` and then run the same CSV replay path:

```bash
cargo run --release -p athenas-pallas --features databento --bin pallas-backtest -- \
  --provider databento \
  --dataset EQUS.MINI \
  --symbol AAPL \
  --schema ohlcv-1d \
  --start 01-01-2025 \
  --end 02-01-2025 \
  --instrument databento:AAPL \
  --initial-balance USD:10000 \
  --yes
```

Set `DATABENTO_API_KEY` in the repo-root `.env` before fetching uncached data. Use `--estimate-only` to check vendor cost without downloading.

Databento cache export preserves the raw OHLCV values returned by the selected market-data schema;
the engine only converts Databento fixed-point integers to decimal CSV text. It does not apply
split or dividend adjustment factors. Adjustment-factor support is a separate future feature, and
`raw_symbol` refers to input symbology rather than adjusted prices.

## Project Layout

| Path | Purpose |
|------|---------|
| `athenas-pallas/` | Rust backtest engine and `pallas-backtest` CLI |
| `tools/athenas-pallas-tools/` | Optional utilities: sweep and resample |
| `trading/` | Direct Python/C++ strategy folders plus shared SDKs in `_sdk/` |
| `data/` | Your local CSVs (empty in git) |
| `athenas-pallas/tests/fixtures/` | CI / golden test data only |

## Examples

```bash
cargo run -p athenas-pallas --example backtest_csv
```

## CLI Tools

Parameter sweeps and CSV utilities live in a separate workspace crate:

```bash
cargo build --release -p athenas-pallas-tools
cargo run --release -p athenas-pallas-tools --bin pallas-sweep -- --help
cargo run --release -p athenas-pallas-tools --bin pallas-resample -- --help
```

## Benchmarks

```bash
cargo bench -p athenas-pallas --bench backtest_hotpath
```

See [`docs/OPTIMIZATION_AUDIT.md`](docs/OPTIMIZATION_AUDIT.md) for the ranked Rust optimization backlog.

## License

MIT OR Apache-2.0.
