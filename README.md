# Athena's Pallas (`athenas-pallas`)

Open-source Rust framework for **event-driven** algorithmic trading: **live**, **paper**, and **backtest** share the same strategy and risk hooks; you swap **connectors** and the [`ExecutionGateway`](athenas-pallas/src/execution/mod.rs).

Instrument metadata, CSV replay, sync backtest hot path, and external strategy IPC live in the single **`athenas-pallas`** crate. Python and C++ strategies sit in the sibling [`../trading/`](../trading/) tree.

## Quickstart

```bash
cd Backtesting-Engine
cargo test -p athenas-pallas
```

### CSV backtest (built-in buy-and-hold)

```bash
cargo run -p athenas-pallas --bin pallas-backtest -- \
  --data data/BTCUSDT_1d.csv \
  --instrument binance:BTCUSDT \
  --initial-balance USDT:10000
```

Use `--release` for the sync replay hot path.

### Python strategy (SMA crossover)

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data data/BTCUSDT_1d.csv \
  --instrument binance:BTCUSDT \
  --initial-balance USDT:10000 \
  --strategy ../trading/strategies/simple_sma/strategy.py \
  --output results.json
```

Protocol details: [`../trading/protocol.md`](../trading/protocol.md). SDK: [`../trading/sdk/pallas_strategy.py`](../trading/sdk/pallas_strategy.py).

Helper scripts: [`scripts/run_sma_backtest.ps1`](scripts/run_sma_backtest.ps1) (Windows) and [`scripts/run_sma_backtest.sh`](scripts/run_sma_backtest.sh) (Unix).

### Example: CSV replay + metrics

```bash
cargo run -p backtest_csv
```

Loads [`data/BTCUSDT_1d.csv`](data/BTCUSDT_1d.csv) via `CsvBarSource`, simulates fills, prints PnL / Sharpe / drawdown.

### Paper trading (live public Binance data + local execution)

```bash
cargo run -p paper_binance
```

Optional control plane when built with `control-server` — see control routes in the example source.

### Live Binance Spot (REST + user stream)

**Warning: this path can spend real funds.** Prefer testnet URLs until you point at mainnet deliberately.

```bash
cargo run -p live_binance
```

Set `BINANCE_API_KEY`, `BINANCE_SECRET`, `BINANCE_BASE_URL`, and `BINANCE_WS_URL` (see example header comments).

## Backtest CLI flags

| Flag | Purpose |
|------|---------|
| `--data` | CSV path (OHLCV, Yahoo `Date,...`, or FX `bid,ask`) |
| `--data-format` | `auto`, `ohlcv`, `yahoo`, `fx` |
| `--instrument` | `exchange:SYMBOL` (e.g. `binance:BTCUSDT`) |
| `--initial-balance` | Repeatable `ASSET:AMOUNT` |
| `--strategy` | Python script, binary, or directory with `strategy.py` / `CMakeLists.txt` |
| `--python` | Python executable (default `python`) |
| `--fee-bps` / `--slippage-bps` | Paper fill knobs |
| `--output` | JSON report path |

Sample instrument metadata: [`backtest.toml.example`](backtest.toml.example).

## Features

| Feature | Enables |
|---------|---------|
| `binance` | Public WebSocket connector |
| `binance-live` | Signed REST + user stream |
| `control-server` | Localhost HTTP control plane |
| `all` | All optional deps |

## Benchmarks

```bash
cargo bench --bench backtest_hotpath
```

Timings are recorded in [`docs/benchmarks.txt`](docs/benchmarks.txt).

## License

Dual-licensed under MIT OR Apache-2.0.
