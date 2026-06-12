# Athena's Pallas

Event-driven algorithmic trading in Rust: **backtest**, **paper**, and **live** share one engine. Swap data sources and execution backends; keep your strategy and risk logic.

- Sync CSV replay hot path (~0.43 µs/bar amortized — see [benchmarks](docs/benchmarks.txt))
- Python and C++ strategies over newline JSON ([protocol](trading/protocol.md))
- `pallas-fetch` for Yahoo / Binance history
- Desktop app (`pallas-app`) — fetch, configure, backtest, chart

## Install

**Requirements:** Rust 1.74+, Python 3 (for Python strategies), optional Node 20+ / pnpm (desktop app).

```bash
git clone https://github.com/DevomB/Athenas-Pallas.git
cd Athenas-Pallas   # or Backtesting-Engine if that is your repo folder name
cargo build --release -p athenas-pallas
```

Run the test suite (fixtures live under `athenas-pallas/tests/fixtures/`, not in your `data/` folder):

```bash
cargo test -p athenas-pallas
```

## Quick demo (no download)

Uses the committed test fixture — good for a first run after clone:

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- --data athenas-pallas/tests/fixtures/data/BTCUSDT_1d.csv --instrument binance:BTCUSDT --initial-balance USDT:10000
```

Python SMA crossover:

```bash
./scripts/run_sma_backtest.sh    # Unix
# or
./scripts/run_sma_backtest.ps1   # Windows
```

## Real market data

Fetch into `data/` (gitignored local workspace):

```bash
cargo run -p athenas-pallas --bin pallas-fetch --features data-fetch -- --provider yahoo --symbol AAPL --interval 1d --days 90 -o data/AAPL_live.csv

cargo run --release -p athenas-pallas --bin pallas-backtest -- --data data/AAPL_live.csv --data-format yahoo --instrument nasdaq:AAPL --initial-balance USD:10000
```

Copy [`backtest.toml.example`](backtest.toml.example) to `backtest.toml` and point `[backtest].data` at your file.

## Desktop app

```bash
cd pallas-app
pnpm install
pnpm tauri dev
```

Build installer: `pnpm tauri build` (WebView2 on Windows).

## Project layout

| Path | Purpose |
|------|---------|
| `athenas-pallas/` | Rust engine, CLI (`pallas-backtest`, `pallas-fetch`) |
| `pallas-app/` | Tauri desktop UI |
| `trading/` | Python/C++ strategies + SDK |
| `data/` | **Your** fetched CSVs (empty in git) |
| `athenas-pallas/tests/fixtures/` | CI / golden test data only |

## Examples

```bash
cargo run -p athenas-pallas --example backtest_csv
cargo run -p athenas-pallas --example paper_binance --features binance,control-server
```

Live Binance (real funds possible): `live_binance` example with `binance-live` feature — see example source for env vars.

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
