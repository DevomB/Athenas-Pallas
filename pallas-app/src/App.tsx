import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { ConfigForm } from "./components/ConfigForm";
import { EquityChart } from "./components/EquityChart";
import { FetchPanel } from "./components/FetchPanel";
import { FillsTable } from "./components/FillsTable";
import { MetricsPanel } from "./components/MetricsPanel";
import { RunPanel } from "./components/RunPanel";
import { defaultConfig, type ConfigDto, type RunResultDto } from "./types";

type Tab = "fetch" | "config" | "run" | "results";

export default function App() {
  const [tab, setTab] = useState<Tab>("config");
  const [config, setConfig] = useState<ConfigDto>(defaultConfig());
  const [running, setRunning] = useState(false);
  const [status, setStatus] = useState("");
  const [result, setResult] = useState<RunResultDto | null>(null);

  useEffect(() => {
    const finished = listen<RunResultDto>("run-finished", (e) => {
      setResult(e.payload);
      setRunning(false);
      setStatus("finished");
      setTab("results");
    });
    const failed = listen<string>("run-failed", (e) => {
      setRunning(false);
      setStatus(`failed: ${e.payload}`);
    });
    return () => {
      finished.then((f) => f());
      failed.then((f) => f());
    };
  }, []);

  useEffect(() => {
    return () => {
      invoke("session_shutdown").catch(() => {});
    };
  }, []);

  return (
    <div className="app">
      <nav className="tabs">
        {(["fetch", "config", "run", "results"] as Tab[]).map((t) => (
          <button
            key={t}
            className={`tab ${tab === t ? "active" : ""}`}
            onClick={() => setTab(t)}
          >
            {t.charAt(0).toUpperCase() + t.slice(1)}
          </button>
        ))}
      </nav>
      <main className="panel">
        {tab === "fetch" && (
          <FetchPanel
            config={config}
            onDataPath={(path) => setConfig({ ...config, data_path: path })}
          />
        )}
        {tab === "config" && (
          <ConfigForm config={config} onChange={setConfig} />
        )}
        {tab === "run" && (
          <RunPanel
            config={config}
            running={running}
            status={status}
            onRunningChange={setRunning}
            onStatus={setStatus}
          />
        )}
        {tab === "results" && (
          <>
            <MetricsPanel report={result?.report ?? null} />
            <EquityChart curve={result?.report.equity_curve ?? []} />
            <FillsTable fills={result?.fills ?? []} />
            <div className="row">
              <button
                disabled={!result}
                onClick={() =>
                  result &&
                  invoke("export_report", {
                    json: result.full_report_json,
                  })
                }
              >
                Export JSON
              </button>
            </div>
          </>
        )}
      </main>
    </div>
  );
}
