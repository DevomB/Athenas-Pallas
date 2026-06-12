import { createChart, type IChartApi, type ISeriesApi, type LineData } from "lightweight-charts";
import { useEffect, useRef } from "react";
import type { EquityPointDto } from "../types";

interface Props {
  curve: EquityPointDto[];
}

export function EquityChart({ curve }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<ISeriesApi<"Line"> | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const chart = createChart(containerRef.current, {
      layout: { background: { color: "#1c212b" }, textColor: "#bdc1c6" },
      grid: { vertLines: { color: "#2a2f3a" }, horzLines: { color: "#2a2f3a" } },
      width: containerRef.current.clientWidth,
      height: 360,
    });
    const series = chart.addLineSeries({ color: "#8ab4f8", lineWidth: 2 });
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

  return <div className="chart" ref={containerRef} />;
}
