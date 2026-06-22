# Algorithm testing guide

See the [README](../README.md) for install and quickstart. This page is the longer copy-paste flow.

## 1. Add data

Files in `data/` are local only (gitignored). Export or copy your market-history CSVs there before running a backtest.

## 2. Backtest (built-in buy-and-hold)

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data athenas-pallas/tests/fixtures/data/EXAMPLE_1d.csv \
  --instrument test:EXAMPLE \
  --initial-balance USD:10000
```

Equity OHLCV from a local export:

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data data/AAPL_live.csv \
  --data-format ohlcv \
  --instrument csv:AAPL \
  --asset-class equity \
  --initial-balance USD:10000
```

## 3. Python strategy

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data athenas-pallas/tests/fixtures/data/EXAMPLE_1d.csv \
  --instrument test:EXAMPLE \
  --initial-balance USD:10000 \
  --strategy simple_sma \
  --output target/report.json
```

Crypto-shaped fixture (explicit base/quote):

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data athenas-pallas/tests/fixtures/data/BTCUSDT_1d.csv \
  --instrument test:BTCUSDT \
  --asset-class crypto \
  --initial-balance USDT:10000 \
  --strategy simple_sma \
  --output target/report.json
```

## 4. TOML config

```bash
cp backtest.toml.example backtest.toml
# edit [backtest].data to your CSV
cargo run --release -p athenas-pallas --bin pallas-backtest -- --config backtest.toml
```

## 5. Merge, sweep, resample

These live in the separate tools crate:

```bash
cargo build --release -p athenas-pallas-tools

# Merge two CSV streams by timestamp
cargo run --release -p athenas-pallas-tools --bin pallas-merge -- \
  --source ohlcv:test:BTCUSDT:data/BTC.csv \
  --source yahoo:test:AAPL:data/AAPL.csv \
  -o data/merged.csv

# Parameter grid from TOML (see sweep.toml.example)
cargo run --release -p athenas-pallas-tools --bin pallas-sweep -- \
  --config backtest.toml --sweep sweep.toml.example -o target/sweep.csv

# Resample bars offline
cargo run --release -p athenas-pallas-tools --bin pallas-resample -- \
  --input data/BTCUSDT_1m.csv --to 30m -o data/BTCUSDT_30m.csv
```

## 6. JSONL event replay

Record events with `backtest::write_events_jsonl`, then replay via `read_events_jsonl` + `replay_events_serial` for deterministic strategy debugging without reloading CSV.

## 7. Golden tests (CI)

```bash
cargo test -p athenas-pallas
cargo test -p athenas-pallas --test external_strategy_golden -- --ignored
cargo test -p athenas-pallas --test cpp_strategy_golden -- --ignored
```

Fixtures: `athenas-pallas/tests/fixtures/`.
