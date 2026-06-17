-- Example readonly_query patterns for CLI/reporting once runs are persisted.
-- Always include tenant_id - DSQL MCP has no parameterized queries; validate IDs in caller code.

-- Top strategies by Sharpe on BTCUSDT (last 30 days)
SELECT
  strategy_path,
  sharpe,
  max_drawdown,
  pnl_pct,
  fill_count,
  created_at
FROM backtest_runs
WHERE tenant_id = 'default'
  AND exchange = 'binance'
  AND symbol = 'BTCUSDT'
  AND created_at >= NOW() - INTERVAL '30 days'
ORDER BY sharpe DESC
LIMIT 20;

-- Compare two runs side-by-side
SELECT run_id, sharpe, sortino, max_drawdown, pnl_pct, wall_time_ms
FROM backtest_runs
WHERE tenant_id = 'default'
  AND run_id IN ('run-abc', 'run-def');

-- Equity curve for reporting/charting.
SELECT ts_unix_ms, equity_quote
FROM backtest_equity_points
WHERE tenant_id = 'default'
  AND run_id = 'run-abc'
ORDER BY ts_unix_ms;
