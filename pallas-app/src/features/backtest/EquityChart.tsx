import {
  createChart,
  type IChartApi,
  type ISeriesApi,
  type LineData,
} from "lightweight-charts";
import { useEffect, useMemo, useRef, useState } from "react";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import type { EquityPointDto } from "@/types";

interface Props {
  curve: EquityPointDto[];
  equityCurveSkipped?: boolean;
  equityCurveDownsampled?: boolean;
  height?: number;
}

function readCssVar(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return value || fallback;
}

function computeDrawdownSeries(curve: EquityPointDto[]): LineData[] {
  let peak = curve[0]?.equity_f64 ?? 0;
  return curve.map((p) => {
    peak = Math.max(peak, p.equity_f64);
    const ddPct = peak > 0 ? ((p.equity_f64 - peak) / peak) * 100 : 0;
    return {
      time: Math.floor(p.ts_unix_ms / 1000) as LineData["time"],
      value: ddPct,
    };
  });
}

export function EquityChart({
  curve,
  equityCurveSkipped,
  equityCurveDownsampled,
  height = 480,
}: Props) {
  const [showDrawdown, setShowDrawdown] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const equitySeriesRef = useRef<ISeriesApi<"Line"> | null>(null);
  const drawdownSeriesRef = useRef<ISeriesApi<"Line"> | null>(null);

  const summary = useMemo(() => {
    if (curve.length === 0) return "No equity curve data";
    const first = curve[0].equity_f64;
    const last = curve[curve.length - 1].equity_f64;
    const change = last - first;
    const changePct = first !== 0 ? (change / first) * 100 : 0;
    return `${curve.length} points, ${first.toFixed(2)} -> ${last.toFixed(2)} (${changePct >= 0 ? "+" : ""}${changePct.toFixed(2)}%)`;
  }, [curve]);

  useEffect(() => {
    if (!containerRef.current) return;
    const chart = createChart(containerRef.current, {
      layout: {
        background: { color: readCssVar("--chart-bg", "#1c212b") },
        textColor: readCssVar("--chart-text", "#bdc1c6"),
      },
      grid: {
        vertLines: { color: readCssVar("--chart-grid", "#2a2f3a") },
        horzLines: { color: readCssVar("--chart-grid", "#2a2f3a") },
      },
      width: containerRef.current.clientWidth,
      height,
    });
    const equitySeries = chart.addLineSeries({
      color: readCssVar("--chart-line", "#8ab4f8"),
      lineWidth: 2,
    });
    const drawdownSeries = chart.addLineSeries({
      color: readCssVar("--destructive", "#f87171"),
      lineWidth: 2,
      visible: false,
      priceScaleId: "drawdown",
    });
    chart.priceScale("drawdown").applyOptions({
      scaleMargins: { top: 0.7, bottom: 0 },
    });
    chartRef.current = chart;
    equitySeriesRef.current = equitySeries;
    drawdownSeriesRef.current = drawdownSeries;

    const onResize = () => {
      if (containerRef.current) {
        chart.applyOptions({ width: containerRef.current.clientWidth });
      }
    };
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("resize", onResize);
      chart.remove();
      chartRef.current = null;
      equitySeriesRef.current = null;
      drawdownSeriesRef.current = null;
    };
  }, [height]);

  useEffect(() => {
    if (!equitySeriesRef.current || !drawdownSeriesRef.current) return;
    const equityData: LineData[] = curve.map((p) => ({
      time: Math.floor(p.ts_unix_ms / 1000) as LineData["time"],
      value: p.equity_f64,
    }));
    equitySeriesRef.current.setData(equityData);
    drawdownSeriesRef.current.setData(computeDrawdownSeries(curve));
    drawdownSeriesRef.current.applyOptions({ visible: showDrawdown });
    chartRef.current?.timeScale().fitContent();
  }, [curve, showDrawdown]);

  if (equityCurveSkipped && curve.length === 0) {
    return (
      <p className="text-sm text-muted-foreground" aria-live="polite">
        Equity curve was not recorded for this run.
      </p>
    );
  }

  return (
    <figure className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <figcaption className="text-sm text-muted-foreground" aria-live="polite">
          {summary}
        </figcaption>
        {curve.length > 0 && (
          <div className="flex items-center gap-2">
            <Switch
              id="drawdown-overlay"
              checked={showDrawdown}
              onCheckedChange={setShowDrawdown}
            />
            <Label htmlFor="drawdown-overlay" className="text-sm font-normal">
              Drawdown overlay
            </Label>
          </div>
        )}
      </div>
      <div
        className="overflow-hidden rounded-lg border bg-card"
        ref={containerRef}
        style={{ height }}
        role="img"
        aria-label={`Equity curve chart. ${summary}`}
      />
      {equityCurveDownsampled && (
        <p className="text-xs text-muted-foreground" aria-live="polite">
          Chart shows a downsampled equity curve (max 2,000 points).
        </p>
      )}
    </figure>
  );
}
