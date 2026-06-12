# Algorithm testing guide

See the [README](../README.md) for install and quickstart. This page is the longer copy-paste flow.

## 1. Fetch data

```bash
cargo run -p athenas-pallas --bin pallas-fetch --features data-fetch -- \
  --provider yahoo --symbol AAPL --interval 1d --days 30 \
  -o data/AAPL_live.csv
```

Files in `data/` are local only (gitignored).

## 2. Backtest (built-in buy-and-hold)

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data data/AAPL_live.csv \
  --data-format yahoo \
  --instrument nasdaq:AAPL \
  --initial-balance USD:10000
```

## 3. Python strategy

```bash
cargo run --release -p athenas-pallas --bin pallas-backtest -- \
  --data athenas-pallas/tests/fixtures/data/BTCUSDT_1d.csv \
  --instrument binance:BTCUSDT \
  --initial-balance USDT:10000 \
  --strategy trading/strategies/simple_sma/strategy.py \
  --output target/report.json
```

## 4. TOML config

```bash
cp backtest.toml.example backtest.toml
# edit [backtest].data to your fetched CSV
cargo run --release -p athenas-pallas --bin pallas-backtest -- --config backtest.toml
```

## 5. Golden tests (CI)

```bash
cargo test -p athenas-pallas
cargo test -p athenas-pallas --test external_strategy_golden -- --ignored
```

Fixtures: `athenas-pallas/tests/fixtures/`.
