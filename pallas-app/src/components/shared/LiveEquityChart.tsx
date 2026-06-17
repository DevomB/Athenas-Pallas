import {
  createChart,
  type IChartApi,
  type ISeriesApi,
  type LineData,
} from "lightweight-charts";
import { useEffect, useMemo, useRef } from "react";
import { usePrefersReducedMotion } from "@/lib/usePrefersReducedMotion";

export interface LiveEquityPoint {
  time: number;
  equity: number;
}

interface Props {
  points: LiveEquityPoint[];
  height?: number;
}

function readCssVar(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return value || fallback;
}

export function LiveEquityChart({ points, height = 280 }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<ISeriesApi<"Line"> | null>(null);
  const reducedMotion = usePrefersReducedMotion();

  const summary = useMemo(() => {
    if (points.length === 0) return "Waiting for equity updates...";
    const first = points[0].equity;
    const last = points[points.length - 1].equity;
    const change = last - first;
    const changePct = first !== 0 ? (change / first) * 100 : 0;
    return `${points.length} ticks | ${first.toFixed(2)} -> ${last.toFixed(2)} (${changePct >= 0 ? "+" : ""}${changePct.toFixed(2)}%)`;
  }, [points]);

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
      timeScale: { timeVisible: true, secondsVisible: true },
    });
    const series = chart.addLineSeries({
      color: readCssVar("--primary", "#d7af54"),
      lineWidth: 2,
    });
    chartRef.current = chart;
    seriesRef.current = series;

    const ro = new ResizeObserver(() => {
      if (containerRef.current) {
        chart.applyOptions({ width: containerRef.current.clientWidth });
      }
    });
    ro.observe(containerRef.current);

    return () => {
      ro.disconnect();
      chart.remove();
      chartRef.current = null;
      seriesRef.current = null;
    };
  }, [height]);

  useEffect(() => {
    if (!seriesRef.current || points.length === 0) return;
    const data: LineData[] = points.map((p) => ({
      time: p.time as LineData["time"],
      value: p.equity,
    }));
    seriesRef.current.setData(data);
    if (!reducedMotion) {
      chartRef.current?.timeScale().fitContent();
    }
  }, [points, reducedMotion]);

  return (
    <div className="flex flex-col gap-2">
      <p className="text-xs text-muted-foreground">{summary}</p>
      <div ref={containerRef} className="w-full rounded-md border motion-reduce:transition-none" />
    </div>
  );
}
