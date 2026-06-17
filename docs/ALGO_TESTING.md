# Algorithm testing guide

See the [README](../README.md) for install and quickstart. This page is the longer copy-paste flow.

## 1. Fetch data

```bash
cargo run -p athenas-pallas --bin pallas-fetch --features data-fetch -- \
  --provider alpha-vantage --asset equity --symbol AAPL --days 30 \
  -o data/AAPL_live.csv
```

Files in `data/` are local only (gitignored). Set `ALPHA_VANTAGE_API_KEY` in your shell or repo-local `.env` before fetching.

## 2. Backtest (built-in buy-and-hold)

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data data/AAPL_live.csv \
  --data-format ohlcv \
  --instrument alpha-vantage:AAPL \
  --initial-balance USD:10000
```

## 3. Python strategy

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data athenas-pallas/tests/fixtures/data/BTCUSDT_1d.csv \
  --instrument alpha-vantage:BTCUSDT \
  --initial-balance USDT:10000 \
  --strategy simple_sma \
  --output target/report.json
```

## 4. TOML config

```bash
cp backtest.toml.example backtest.toml
# edit [backtest].data to your fetched CSV
cargo run --release -p athenas-pallas --bin pallas-backtest -- --config backtest.toml
```

## 6. Merge, sweep, stress

```bash
# Merge two CSV streams by timestamp
cargo run -p athenas-pallas --bin pallas-merge -- \
  --source ohlcv:alpha-vantage:BTCUSDT:data/BTC.csv \
  --source ohlcv:alpha-vantage:AAPL:data/AAPL.csv \
  -o data/merged.csv

# Parameter grid from TOML
cargo run -p athenas-pallas --bin pallas-sweep -- \
  --config backtest.toml --sweep sweep.toml -o target/sweep.csv

# Large-run throughput smoke test
cargo run --release -p athenas-pallas --example stress_backtest -- 100000
```

## 7. JSONL event replay

Record events with `backtest::write_events_jsonl`, then replay via `read_events_jsonl` + `replay_events_serial` for deterministic strategy debugging without reloading CSV.

## 5. Golden tests (CI)

```bash
cargo test -p athenas-pallas
cargo test -p athenas-pallas --test external_strategy_golden -- --ignored
```

Fixtures: `athenas-pallas/tests/fixtures/`.
