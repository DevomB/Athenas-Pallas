import { invoke } from "@tauri-apps/api/core";
import { lazy, Suspense, useState } from "react";
import { ConfigForm } from "./components/ConfigForm";
import { FetchPanel } from "./components/FetchPanel";
import { FillsTable } from "./components/FillsTable";
import { MetricsPanel } from "./components/MetricsPanel";
import { RunPanel } from "./components/RunPanel";
import { StatusBanner } from "./components/ui/StatusBanner";
import { useBacktestSession, type Tab } from "./hooks/useBacktestSession";
import { defaultConfig, type ConfigDto } from "./types";

const EquityChart = lazy(() =>
  import("./components/EquityChart").then((m) => ({ default: m.EquityChart })),
);

const tabs: Array<{ id: Tab; label: string; description: string }> = [
  { id: "fetch", label: "Data", description: "Download CSV bars" },
  { id: "config", label: "Config", description: "Backtest settings" },
  { id: "run", label: "Run", description: "Start or stop worker" },
  { id: "results", label: "Results", description: "Metrics and fills" },
];

export default function App() {
  const {
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
  } = useBacktestSession();

  const [config, setConfig] = useState<ConfigDto>(defaultConfig());
  const [exportStatus, setExportStatus] = useState("");

  async function onExport() {
    if (!result) return;
    setExportStatus("");
    try {
      await invoke("export_report", { json: result.full_report_json });
      setExportStatus("Report exported.");
    } catch (e) {
      setExportStatus(`error: ${e}`);
    }
  }

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
        <nav className="tabs" role="tablist" aria-label="Workflow">
          {tabs.map((item) => (
            <button
              key={item.id}
              type="button"
              role="tab"
              id={`tab-${item.id}`}
              aria-selected={tab === item.id}
              aria-controls={`panel-${item.id}`}
              className={`tab ${tab === item.id ? "active" : ""}`}
              onClick={() => setTab(item.id)}
            >
              <span>{item.label}</span>
              <small>{item.description}</small>
            </button>
          ))}
        </nav>
        <div className="run-state" aria-live="polite">
          <span
            className={`status-dot ${running ? "live" : ""}`}
            aria-hidden="true"
          />
          <div>
            <strong>{running ? (stopping ? "Stopping" : "Running") : "Idle"}</strong>
            <small>
              {config.exchange}:{config.symbol}
            </small>
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
            {running && (
              <span className="header-run-badge" aria-live="polite">
                {stopping ? "Stopping" : "Running"}
              </span>
            )}
            <span>{config.asset_class}</span>
            <span>{config.data_format}</span>
            <span>{config.periods_per_year} periods/year</span>
          </div>
        </header>
        <section
          className="panel"
          role="tabpanel"
          id={`panel-${tab}`}
          aria-labelledby={`tab-${tab}`}
        >
          {tab === "fetch" && (
            <FetchPanel
              config={config}
              onConfigChange={setConfig}
              onNavigate={setTab}
            />
          )}
          {tab === "config" && (
            <ConfigForm config={config} onChange={setConfig} />
          )}
          {tab === "run" && (
            <RunPanel
              config={config}
              running={running}
              stopping={stopping}
              status={status}
              error={error}
              onRunningChange={setRunning}
              onStoppingChange={setStopping}
              onStatus={setStatus}
              onClearError={clearError}
              equityCurveSkipped={result?.equity_curve_skipped}
              equityCurveDownsampled={result?.equity_curve_downsampled}
            />
          )}
          {tab === "results" && (
            <div className="results-stack">
              <MetricsPanel
                report={result?.report ?? null}
                equityCurveSkipped={result?.equity_curve_skipped}
                equityCurveDownsampled={result?.equity_curve_downsampled}
              />
              <Suspense
                fallback={<p className="status">Loading chart...</p>}
              >
                <EquityChart
                  curve={result?.report.equity_curve ?? []}
                  equityCurveSkipped={result?.equity_curve_skipped}
                  equityCurveDownsampled={result?.equity_curve_downsampled}
                />
              </Suspense>
              <FillsTable fills={result?.fills ?? []} />
              <div className="row actions-row">
                <button type="button" disabled={!result} onClick={onExport}>
                  Export JSON
                </button>
              </div>
              {exportStatus && (
                <StatusBanner
                  message={exportStatus}
                  variant={exportStatus.startsWith("error:") ? "error" : "success"}
                  onDismiss={() => setExportStatus("")}
                />
              )}
            </div>
          )}
        </section>
      </main>
    </div>
  );
}
