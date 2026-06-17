# Athena's Pallas

Event-driven algorithmic trading in Rust: **backtest**, **paper**, and **live** share one engine. Swap data sources and execution backends; keep your strategy and risk logic.

- Sync CSV replay hot path (~0.43 microseconds/bar amortized; see [benchmarks](docs/benchmarks.txt))
- Python and C++ strategies over newline JSON ([protocol](trading/protocol.md))
- `pallas-fetch` for Yahoo / Binance history
- Desktop app (`pallas-app`) for fetch, configure, backtest, and chart workflows

## Install

**Requirements:** Rust 1.85+, Python 3 for Python strategies, optional Node 20+ / pnpm for the desktop app.

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
cargo run --release -p athenas-pallas --bin pallas-backtest -- --data athenas-pallas/tests/fixtures/data/BTCUSDT_1d.csv --instrument binance:BTCUSDT --initial-balance USDT:10000
```

Direct strategy-name resolution:

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- --data athenas-pallas/tests/fixtures/data/BTCUSDT_1d.csv --instrument binance:BTCUSDT --initial-balance USDT:10000 --strategy simple_sma
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

## Real Market Data

Fetch into `data/` (gitignored local workspace):

```bash
cargo run -p athenas-pallas --bin pallas-fetch --features data-fetch -- --provider yahoo --symbol AAPL --interval 1d --days 90 -o data/AAPL_live.csv

cargo run --release -p athenas-pallas --bin pallas-backtest -- --data data/AAPL_live.csv --data-format yahoo --instrument nasdaq:AAPL --initial-balance USD:10000
```

Copy [`backtest.toml.example`](backtest.toml.example) to `backtest.toml` and point `[backtest].data` at your file.

## Desktop App

```bash
cd pallas-app
pnpm install
pnpm tauri dev
```

Build installer: `pnpm tauri build` (WebView2 on Windows).

## Project Layout

| Path | Purpose |
|------|---------|
| `athenas-pallas/` | Rust engine, CLI (`pallas-backtest`, `pallas-fetch`) |
| `pallas-app/` | Tauri desktop UI |
| `trading/` | Direct Python/C++ strategy folders plus shared SDKs in `_sdk/` |
| `data/` | Your fetched CSVs (empty in git) |
| `athenas-pallas/tests/fixtures/` | CI / golden test data only |

## Examples

```bash
cargo run -p athenas-pallas --example backtest_csv
cargo run -p athenas-pallas --example paper_binance --features binance,control-server
```

Live Binance can trade real funds. Use the `live_binance` example with the `binance-live` feature and read the example source for required environment variables.

## Features

| Cargo feature | Enables |
|---------------|---------|
| `data-fetch` | `pallas-fetch` (Yahoo / Binance) |
| `binance` | Public WebSocket connector |
| `binance-live` | Signed REST + user stream |
| `control-server` | Localhost HTTP control plane |
| `all` | All optional deps |

## Benchmarks

```bash
cargo bench -p athenas-pallas --bench backtest_hotpath
```

## License

MIT OR Apache-2.0.
