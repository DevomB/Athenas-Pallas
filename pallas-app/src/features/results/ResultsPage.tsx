import { invoke } from "@tauri-apps/api/core";
import { lazy, Suspense } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import type { RunResultDto, TradingStateDto } from "@/types";
import type { PositionsSnapshotDto } from "@/types";
import { MetricsPanel } from "@/features/backtest/MetricsPanel";
import { FillsTable } from "@/features/backtest/FillsTable";

const EquityChart = lazy(() =>
  import("@/features/backtest/EquityChart").then((m) => ({
    default: m.EquityChart,
  })),
);

interface Props {
  result: RunResultDto | null;
  resultSummary?: string | null;
  tradingState: TradingStateDto;
  tradingSnapshot: PositionsSnapshotDto | null;
  onNavigate: (route: "quick-start" | "backtest" | "paper" | "live") => void;
}

export function ResultsPage({
  result,
  resultSummary,
  tradingState,
  tradingSnapshot,
  onNavigate,
}: Props) {
  async function onExport() {
    if (!result) return;
    try {
      await invoke("export_report", { json: result.full_report_json });
      toast.success("Report exported");
    } catch (e) {
      toast.error(String(e));
    }
  }

  if (!result && tradingState.mode === "idle") {
    return (
      <Empty>
        <EmptyHeader>
          <EmptyTitle>No results yet</EmptyTitle>
          <EmptyDescription>
            Run a backtest from Quick Start or the Backtest tab to see metrics,
            equity curve, and fills here.
          </EmptyDescription>
        </EmptyHeader>
        <EmptyContent className="flex flex-wrap gap-2">
          <Button onClick={() => onNavigate("quick-start")}>Quick Start</Button>
          <Button variant="secondary" onClick={() => onNavigate("backtest")}>
            Configure backtest
          </Button>
        </EmptyContent>
      </Empty>
    );
  }

  return (
    <div className="flex flex-col gap-6" aria-live="polite" aria-atomic="false">
      {resultSummary && result && (
        <Card className="border-primary/40 bg-primary/5">
          <CardHeader>
            <CardTitle className="text-base">{resultSummary}</CardTitle>
            <CardDescription>
              {result.report.equity_curve.length} equity points ·{" "}
              {result.report.wall_time_ms} ms wall time
            </CardDescription>
          </CardHeader>
        </Card>
      )}

      {tradingState.mode !== "idle" && tradingSnapshot && (
        <Card>
          <CardHeader>
            <CardTitle>Active session snapshot</CardTitle>
            <CardDescription>
              {tradingState.mode === "paper" ? "Paper" : "Live"} ·{" "}
              {tradingState.instrument}
            </CardDescription>
          </CardHeader>
          <CardContent className="flex flex-wrap gap-4 text-sm">
            <div>
              <span className="text-muted-foreground">Equity: </span>
              <strong>{tradingSnapshot.equity}</strong>
            </div>
            {tradingSnapshot.mark_price && (
              <div>
                <span className="text-muted-foreground">Mark: </span>
                <strong>{tradingSnapshot.mark_price}</strong>
              </div>
            )}
            <div>
              <span className="text-muted-foreground">Positions: </span>
              <strong>{tradingSnapshot.positions.length}</strong>
            </div>
            <Button
              variant="link"
              className="h-auto p-0"
              onClick={() =>
                onNavigate(tradingState.mode === "paper" ? "paper" : "live")
              }
            >
              Open session
            </Button>
          </CardContent>
        </Card>
      )}

      {result ? (
        <>
          <MetricsPanel
            report={result.report}
            equityCurveSkipped={result.equity_curve_skipped}
            equityCurveDownsampled={result.equity_curve_downsampled}
          />
          <Suspense
            fallback={
              <p className="text-sm text-muted-foreground">Loading chart…</p>
            }
          >
            <EquityChart
              curve={result.report.equity_curve}
              equityCurveSkipped={result.equity_curve_skipped}
              equityCurveDownsampled={result.equity_curve_downsampled}
            />
          </Suspense>
          <FillsTable fills={result.fills} />
          <Button variant="secondary" onClick={onExport}>
            Export JSON report
          </Button>
        </>
      ) : (
        <p className="text-sm text-muted-foreground">
          No backtest results in this session. Start a paper or live session, or
          run a backtest to populate this hub.
        </p>
      )}
    </div>
  );
}
