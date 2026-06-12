#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo build --release --bin pallas-backtest

cargo run --release --bin pallas-backtest -- \
  --data data/BTCUSDT_1d.csv \
  --instrument binance:BTCUSDT \
  --initial-balance USDT:10000 \
  --strategy ../trading/strategies/simple_sma/strategy.py \
  --output target/sma_report.json \
  --verbose
