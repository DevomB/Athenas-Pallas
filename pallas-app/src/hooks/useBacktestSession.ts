import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import type { RunResultDto } from "../types";

export type Tab = "fetch" | "config" | "run" | "results";

export function useBacktestSession() {
  const [tab, setTab] = useState<Tab>("config");
  const [running, setRunning] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [result, setResult] = useState<RunResultDto | null>(null);

  useEffect(() => {
    const unsubs: Array<() => void> = [];

    listen<RunResultDto>("run-finished", (e) => {
      setResult(e.payload);
      setRunning(false);
      setStopping(false);
      setStatus("finished");
      setError("");
      setTab("results");
    }).then((fn) => unsubs.push(fn));

    listen<string>("run-failed", (e) => {
      setRunning(false);
      setStopping(false);
      setError(e.payload);
      setStatus(`failed: ${e.payload}`);
      setTab("run");
    }).then((fn) => unsubs.push(fn));

    listen<string>("run-progress", (e) => {
      setStatus(String(e.payload));
    }).then((fn) => unsubs.push(fn));

    return () => {
      unsubs.forEach((fn) => fn());
    };
  }, []);

  useEffect(() => {
    return () => {
      invoke("session_shutdown").catch(() => {});
    };
  }, []);

  const clearError = useCallback(() => setError(""), []);

  return {
    tab,
    setTab,
    running,
    setRunning,
    stopping,
    setStopping,
    status,
    setStatus,
    error,
    clearError,
    result,
    setResult,
  };
}
