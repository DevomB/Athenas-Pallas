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

const tabs: Array<{ id: Tab; label: string; description: string }> = [
  { id: "fetch", label: "Data", description: "Download CSV bars" },
  { id: "config", label: "Config", description: "Backtest settings" },
  { id: "run", label: "Run", description: "Start or stop worker" },
  { id: "results", label: "Results", description: "Metrics and fills" },
];

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
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">P</div>
          <div>
            <h1>Pallas</h1>
            <p>Backtesting workbench</p>
          </div>
        </div>
        <nav className="tabs" aria-label="Workflow">
          {tabs.map((item) => (
            <button
              key={item.id}
              className={`tab ${tab === item.id ? "active" : ""}`}
              onClick={() => setTab(item.id)}
            >
              <span>{item.label}</span>
              <small>{item.description}</small>
            </button>
          ))}
        </nav>
        <div className="run-state">
          <span className={`status-dot ${running ? "live" : ""}`} />
          <div>
            <strong>{running ? "Running" : "Idle"}</strong>
            <small>{config.exchange}:{config.symbol}</small>
          </div>
        </div>
      </aside>
      <main className="workspace">
        <header className="workspace-header">
          <div>
            <p className="eyebrow">Backtest setup</p>
            <h2>{tabs.find((item) => item.id === tab)?.label}</h2>
          </div>
          <div className="header-summary">
            <span>{config.asset_class}</span>
            <span>{config.data_format}</span>
            <span>{config.periods_per_year} periods/year</span>
          </div>
        </header>
        <section className="panel">
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
            <div className="results-stack">
              <MetricsPanel report={result?.report ?? null} />
              <EquityChart curve={result?.report.equity_curve ?? []} />
              <FillsTable fills={result?.fills ?? []} />
              <div className="row actions-row">
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
            </div>
          )}
        </section>
      </main>
    </div>
  );
}
