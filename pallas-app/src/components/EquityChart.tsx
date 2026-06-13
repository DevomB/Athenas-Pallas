import {
  createChart,
  type IChartApi,
  type ISeriesApi,
  type LineData,
} from "lightweight-charts";
import { useEffect, useMemo, useRef } from "react";
import type { EquityPointDto } from "../types";

interface Props {
  curve: EquityPointDto[];
  equityCurveSkipped?: boolean;
  equityCurveDownsampled?: boolean;
}

function readCssVar(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return value || fallback;
}

export function EquityChart({
  curve,
  equityCurveSkipped,
  equityCurveDownsampled,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<ISeriesApi<"Line"> | null>(null);

  const summary = useMemo(() => {
    if (curve.length === 0) return "No equity curve data";
    const first = curve[0].equity_f64;
    const last = curve[curve.length - 1].equity_f64;
    const change = last - first;
    const changePct = first !== 0 ? (change / first) * 100 : 0;
    return `${curve.length} points, ${first.toFixed(2)} → ${last.toFixed(2)} (${changePct >= 0 ? "+" : ""}${changePct.toFixed(2)}%)`;
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
      height: 360,
    });
    const series = chart.addLineSeries({
      color: readCssVar("--chart-line", "#8ab4f8"),
      lineWidth: 2,
    });
    chartRef.current = chart;
    seriesRef.current = series;

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
      seriesRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (!seriesRef.current) return;
    const data: LineData[] = curve.map((p) => ({
      time: Math.floor(p.ts_unix_ms / 1000) as LineData["time"],
      value: p.equity_f64,
    }));
    seriesRef.current.setData(data);
    chartRef.current?.timeScale().fitContent();
  }, [curve]);

  if (equityCurveSkipped && curve.length === 0) {
    return (
      <p className="status" aria-live="polite">
        Equity curve was not recorded for this run.
      </p>
    );
  }

  return (
    <figure className="chart-figure">
      <figcaption className="chart-summary" aria-live="polite">
        {summary}
      </figcaption>
      <div
        className="chart"
        ref={containerRef}
        role="img"
        aria-label={`Equity curve chart. ${summary}`}
      />
      {equityCurveDownsampled && (
        <p className="chart-footnote" aria-live="polite">
          Chart shows a downsampled equity curve (max 2,000 points).
        </p>
      )}
    </figure>
  );
}
