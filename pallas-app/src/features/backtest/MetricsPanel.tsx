import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import type { BacktestReportDto } from "@/types";
import { cn } from "@/lib/utils";

interface Props {
  report: BacktestReportDto | null;
  equityCurveSkipped?: boolean;
  equityCurveDownsampled?: boolean;
}

function MetricCard({
  label,
  value,
  highlight,
  positive,
}: {
  label: string;
  value: string;
  highlight?: boolean;
  positive?: boolean;
}) {
  return (
    <Card>
      <CardHeader className="pb-1">
        <CardDescription>{label}</CardDescription>
        <CardTitle
          className={cn(
            highlight && "text-2xl",
            positive === true && "text-emerald-400",
            positive === false && "text-red-400",
          )}
        >
          {value}
        </CardTitle>
      </CardHeader>
    </Card>
  );
}

export function MetricsPanel({
  report,
  equityCurveSkipped,
  equityCurveDownsampled,
}: Props) {
  if (!report) {
    return (
      <p className="text-sm text-muted-foreground" aria-live="polite">
        Run a backtest to see metrics.
      </p>
    );
  }

  const pnlPositive = report.pnl >= 0;

  return (
    <div className="flex flex-col gap-4" role="region" aria-live="polite" aria-label="Backtest metrics">
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        <MetricCard
          label="Net PnL"
          value={report.pnl.toFixed(2)}
          highlight
          positive={pnlPositive}
        />
        <MetricCard
          label="Return"
          value={`${(report.pnl_pct * 100).toFixed(2)}%`}
          highlight
          positive={pnlPositive}
        />
        <MetricCard
          label="Max drawdown"
          value={`${(report.max_drawdown * 100).toFixed(2)}%`}
          positive={false}
        />
      </div>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
        <MetricCard label="Sharpe" value={report.sharpe.toFixed(3)} />
        <MetricCard label="Sortino" value={report.sortino.toFixed(3)} />
        <MetricCard
          label="Win rate"
          value={`${(report.win_rate * 100).toFixed(1)}%`}
        />
        <MetricCard
          label="Profit factor"
          value={report.profit_factor.toFixed(2)}
        />
        <MetricCard
          label="Closed trades"
          value={String(report.closed_trades)}
        />
      </div>
      <div className="flex flex-wrap gap-2">
        <Badge variant="secondary">{report.fill_count} fills</Badge>
        <Badge variant="outline">{report.wall_time_ms} ms wall time</Badge>
      </div>
      {(equityCurveSkipped || equityCurveDownsampled) && (
        <p className="text-xs text-muted-foreground" aria-live="polite">
          {equityCurveSkipped && "Equity curve was not recorded for this run. "}
          {equityCurveDownsampled &&
            "Chart shows a downsampled equity curve (max 2,000 points)."}
        </p>
      )}
    </div>
  );
}
