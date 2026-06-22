#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DEMO_CSV="athenas-pallas/tests/fixtures/data/EXAMPLE_1d.csv"

cargo build --release --bin pallas-backtest

cargo run --release --bin pallas-backtest -- \
  --data "$DEMO_CSV" \
  --instrument test:EXAMPLE \
  --initial-balance USD:10000 \
  --strategy simple_sma \
  --output target/sma_report.json \
  --verbose
