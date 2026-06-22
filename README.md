# Athena's Pallas

[![CI](https://github.com/DevomB/Athenas-Pallas/actions/workflows/ci.yml/badge.svg)](https://github.com/DevomB/Athenas-Pallas/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/athenas-pallas.svg)](https://crates.io/crates/athenas-pallas)
[![docs.rs](https://img.shields.io/docsrs/athenas-pallas)](https://docs.rs/athenas-pallas)
[![license](https://img.shields.io/crates/l/athenas-pallas.svg)](#license)

Event-driven algorithmic trading in Rust: **backtest**, **paper**, and **live** share one engine. Swap data sources and execution backends; keep your strategy and risk logic.

- Sync CSV replay hot path (sub-microsecond/bar amortized for a noop strategy; ~0.43–0.60 us/bar depending on host — see [benchmarks](docs/benchmarks.txt))
- Python and C++ strategies over newline JSON ([protocol](trading/protocol.md))
- Local CSV, pbar, and strategy-driven backtest workflows

## Install

**Requirements:** Rust 1.85+, Python 3 for Python strategies.

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

Built-in buy-and-hold over the committed BTCUSDT fixture:

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- --data athenas-pallas/tests/fixtures/data/BTCUSDT_1d.csv --instrument csv:BTCUSDT --initial-balance USDT:10000
```

Direct strategy-name resolution:

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- --data athenas-pallas/tests/fixtures/data/BTCUSDT_1d.csv --instrument csv:BTCUSDT --initial-balance USDT:10000 --strategy simple_sma
```

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
    cpp/json.hpp
  simple_sma/
    strategy.py
  simple_sma_cpp/
    CMakeLists.txt
    main.cpp
```

Detection order:

1. Directory with `CMakeLists.txt`: build with CMake and run the compiled binary.
2. Directory with `strategy.py` or `main.py`: run with the configured Python executable.
3. `.py` file: run with Python.
4. Other file path: run as a binary.

Legacy paths such as `trading/strategies/simple_sma/strategy.py` are still resolved for compatibility, but new configs should use `strategy = "simple_sma"`.

## Market Data

Put local CSV exports into `data/` (gitignored local workspace), then point the backtest at the file:

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- --data data/AAPL_live.csv --data-format ohlcv --instrument csv:AAPL --asset-class equity --initial-balance USD:10000
```

Copy [`backtest.toml.example`](backtest.toml.example) to `backtest.toml` and point `[backtest].data` at your file.

### Databento Historical Fetch

Enable the optional Databento Rust client to pull OHLCV bars into the same CSV format:

```bash
$env:DATABENTO_API_KEY="YOUR_KEY"  # PowerShell
cargo run --release -p athenas-pallas --features databento --bin pallas-databento-fetch -- `
  --dataset XNAS.ITCH `
  --symbol AAPL `
  --schema ohlcv-1d `
  --start 2024-01-01 `
  --end 2024-02-01 `
  --output data/AAPL_databento.csv

cargo run --release -p athenas-pallas --bin pallas-backtest -- `
  --data data/AAPL_databento.csv `
  --data-format ohlcv `
  --instrument databento:AAPL `
  --asset-class equity `
  --initial-balance USD:10000
```

## Project Layout

| Path | Purpose |
|------|---------|
| `athenas-pallas/` | Rust engine and CLI tools |
| `trading/` | Direct Python/C++ strategy folders plus shared SDKs in `_sdk/` |
| `data/` | Your local CSVs (empty in git) |
| `athenas-pallas/tests/fixtures/` | CI / golden test data only |

## Examples

```bash
cargo run -p athenas-pallas --example backtest_csv
cargo run -p athenas-pallas --example paper_binance --features binance,control-server
```

Binance execution examples are optional broker-specific demos only; do not use them unless you intentionally want Binance order routing.

## Features

| Cargo feature | Enables |
|---------------|---------|
| `binance` | Public WebSocket connector |
| `binance-live` | Signed REST + user stream |
| `control-server` | Localhost HTTP control plane |
| `databento` | Databento historical OHLCV fetch CLI |
| `all` | All optional deps |

## Benchmarks

```bash
cargo bench -p athenas-pallas --bench backtest_hotpath
```

See [`docs/OPTIMIZATION_AUDIT.md`](docs/OPTIMIZATION_AUDIT.md) for the ranked Rust optimization backlog.

## License

MIT OR Apache-2.0.
