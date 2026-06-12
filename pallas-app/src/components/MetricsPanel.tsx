import type { BacktestReportDto } from "../types";

interface Props {
  report: BacktestReportDto | null;
}

export function MetricsPanel({ report }: Props) {
  if (!report) {
    return <p className="status">Run a backtest to see metrics.</p>;
  }
  const items = [
    ["PnL", report.pnl.toFixed(2)],
    ["PnL %", (report.pnl_pct * 100).toFixed(2) + "%"],
    ["Sharpe", report.sharpe.toFixed(3)],
    ["Sortino", report.sortino.toFixed(3)],
    ["Max DD", (report.max_drawdown * 100).toFixed(2) + "%"],
    ["Fills", String(report.fill_count)],
    ["Wall ms", String(report.wall_time_ms)],
  ];
  return (
    <div className="metrics">
      {items.map(([label, value]) => (
        <div className="metric" key={label}>
          <div className="label">{label}</div>
          <div className="value">{value}</div>
        </div>
      ))}
    </div>
  );
}
