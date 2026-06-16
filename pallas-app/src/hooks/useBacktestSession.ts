import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { mapUserError } from "@/lib/errorMessages";
import type { AppRoute, RunResultDto } from "../types";

export function useBacktestSession(onNavigate?: (route: AppRoute) => void) {
  const [running, setRunning] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [logLines, setLogLines] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [result, setResult] = useState<RunResultDto | null>(null);
  const [resultSummary, setResultSummary] = useState<string | null>(null);
  const appendLog = useCallback((line: string) => {
    setLogLines((prev) => {
      const next = [...prev, line];
      return next.length > 500 ? next.slice(-500) : next;
    });
  }, []);

  useEffect(() => {
    const unsubs: Array<() => void> = [];

    listen<RunResultDto>("run-finished", (e) => {
      const report = e.payload.report;
      const summary = `Your strategy ${report.pnl >= 0 ? "made" : "lost"} ${Math.abs(report.pnl).toFixed(2)} (${(report.pnl_pct * 100).toFixed(2)}%) across ${report.equity_curve.length} equity points`;
      setResultSummary(summary);
      setResult(e.payload);
      setRunning(false);
      setStopping(false);
      appendLog("Backtest finished successfully.");
      setError("");
      toast.success("Backtest complete", {
        description: `PnL: ${e.payload.report.pnl.toFixed(2)}`,
      });
      onNavigate?.("results");
    }).then((fn) => unsubs.push(fn));

    listen<string>("run-failed", (e) => {
      const message = mapUserError(e.payload);
      setRunning(false);
      setStopping(false);
      setError(message);
      appendLog(`Failed: ${message}`);
      toast.error("Backtest failed", { description: message });
    }).then((fn) => unsubs.push(fn));

    listen<string>("run-progress", (e) => {
      appendLog(String(e.payload));
    }).then((fn) => unsubs.push(fn));

    return () => {
      unsubs.forEach((fn) => fn());
    };
  }, [appendLog, onNavigate]);

  useEffect(() => {
    return () => {
      invoke("session_shutdown").catch(() => {});
    };
  }, []);

  const clearError = useCallback(() => setError(""), []);
  const clearLog = useCallback(() => setLogLines([]), []);

  return {
    running,
    setRunning,
    stopping,
    setStopping,
    logLines,
    appendLog,
    clearLog,
    error,
    clearError,
    result,
    setResult,
    resultSummary,
    setResultSummary,
  };
}
