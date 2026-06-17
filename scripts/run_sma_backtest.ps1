$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$DemoCsv = "athenas-pallas/tests/fixtures/data/BTCUSDT_1d.csv"

cargo build --release --bin pallas-backtest

cargo run --release --bin pallas-backtest -- `
  --data $DemoCsv `
  --instrument binance:BTCUSDT `
  --initial-balance USDT:10000 `
  --strategy simple_sma `
  --output target/sma_report.json `
  --verbose
