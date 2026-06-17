-- Aurora DSQL schema for Backtesting-Engine run history.
-- Maps Rust types: BacktestReport, FillRecord, EquityPoint (athenas-pallas).
--
-- Apply with the aurora-dsql MCP `transact` tool - one statement per call.
-- Example: transact(["CREATE TABLE IF NOT EXISTS backtest_runs (...)"])
--
-- DSQL rules followed:
--   - tenant_id on every table (multi-tenant isolation)
--   - DECIMAL stored as TEXT (matches engine JSON string precision)
--   - one DDL statement per transaction
--   - CREATE INDEX ASYNC in separate transactions

-- 1) Run metadata + summary metrics (one row per backtest execution)
CREATE TABLE IF NOT EXISTS backtest_runs (
    run_id          TEXT PRIMARY KEY,
    tenant_id       TEXT NOT NULL,
  exchange          TEXT NOT NULL,
  symbol            TEXT NOT NULL,
  asset_class       TEXT NOT NULL,
  strategy_path     TEXT,
  data_path         TEXT NOT NULL,
  fee_bps           TEXT NOT NULL,
  slippage_bps      TEXT NOT NULL,
  half_spread_bps   TEXT NOT NULL,
  periods_per_year  DOUBLE PRECISION NOT NULL,
  pnl               TEXT NOT NULL,
  pnl_pct           TEXT NOT NULL,
  max_drawdown      DOUBLE PRECISION NOT NULL,
  sharpe            DOUBLE PRECISION NOT NULL,
  sortino           DOUBLE PRECISION NOT NULL,
  fill_count        BIGINT NOT NULL,
  wall_time_ms      BIGINT NOT NULL,
  created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 2) Equity curve points (many rows per run; downsampled in GUI, full series here)
CREATE TABLE IF NOT EXISTS backtest_equity_points (
    run_id          TEXT NOT NULL,
    tenant_id       TEXT NOT NULL,
  ts_unix_ms        BIGINT NOT NULL,
  equity_quote      TEXT NOT NULL,
  PRIMARY KEY (run_id, ts_unix_ms)
);

-- 3) Fill blotter (optional per run)
CREATE TABLE IF NOT EXISTS backtest_fills (
    run_id          TEXT NOT NULL,
    tenant_id       TEXT NOT NULL,
  fill_seq          INTEGER NOT NULL,
  ts_unix_ms        BIGINT NOT NULL,
  side              TEXT NOT NULL,
  qty               TEXT NOT NULL,
  price             TEXT NOT NULL,
  fee               TEXT NOT NULL,
  PRIMARY KEY (run_id, fill_seq)
);

-- Indexes (each in its own transact call)
CREATE INDEX ASYNC idx_runs_tenant_created ON backtest_runs (tenant_id, created_at DESC);
CREATE INDEX ASYNC idx_runs_tenant_symbol ON backtest_runs (tenant_id, exchange, symbol);
CREATE INDEX ASYNC idx_runs_tenant_sharpe ON backtest_runs (tenant_id, sharpe DESC);
CREATE INDEX ASYNC idx_equity_tenant_run ON backtest_equity_points (tenant_id, run_id);
CREATE INDEX ASYNC idx_fills_tenant_run ON backtest_fills (tenant_id, run_id);
