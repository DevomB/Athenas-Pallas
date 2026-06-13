import { invoke } from "@tauri-apps/api/core";
import { validateConfig } from "../lib/configValidation";
import type { ConfigDto } from "../types";
import { StatusBanner } from "./ui/StatusBanner";

interface Props {
  config: ConfigDto;
  running: boolean;
  stopping: boolean;
  status: string;
  error: string;
  onRunningChange: (v: boolean) => void;
  onStoppingChange: (v: boolean) => void;
  onStatus: (s: string) => void;
  onClearError: () => void;
  equityCurveSkipped?: boolean;
  equityCurveDownsampled?: boolean;
}

export function RunPanel({
  config,
  running,
  stopping,
  status,
  error,
  onRunningChange,
  onStoppingChange,
  onStatus,
  onClearError,
  equityCurveSkipped,
  equityCurveDownsampled,
}: Props) {
  const validationError = validateConfig(config);
  const statusIsError = status.startsWith("error:") || status.startsWith("failed:");

  async function start() {
    if (validationError) return;
    onClearError();
    onRunningChange(true);
    onStoppingChange(false);
    onStatus("starting...");
    try {
      await invoke("run_backtest", { config });
      onStatus("running on worker thread...");
    } catch (e) {
      onRunningChange(false);
      onStatus(`error: ${e}`);
    }
  }

  async function stop() {
    onStoppingChange(true);
    onStatus("stopping...");
    try {
      await invoke("stop_run");
      onStatus("stop requested");
    } catch (e) {
      onStoppingChange(false);
      onStatus(`error: ${e}`);
    }
  }

  const workerLabel = stopping
    ? "Stopping backtest worker..."
    : running
      ? "Backtest worker is active."
      : "Worker is idle.";

  return (
    <div className="run-screen">
      {error && (
        <StatusBanner
          message={error}
          variant="error"
          onDismiss={onClearError}
        />
      )}

      {validationError && !running && (
        <StatusBanner message={validationError} variant="error" />
      )}

      <section className="run-hero">
        <div>
          <p className="eyebrow">Ready to run</p>
          <h3>
            {config.exchange}:{config.symbol}
          </h3>
          <p>
            {config.asset_class} using {config.data_format} data with{" "}
            {config.fee_bps} bps fees and {config.slippage_bps} bps slippage.
          </p>
        </div>
        <div className="run-buttons">
          <button
            type="button"
            disabled={running || stopping || !!validationError}
            onClick={start}
          >
            Start
          </button>
          <button
            type="button"
            className="danger"
            disabled={!running || stopping}
            onClick={stop}
          >
            {stopping ? "Stopping..." : "Stop"}
          </button>
        </div>
      </section>

      <div className="summary-grid">
        <div>
          <span>Data</span>
          <strong>{config.data_path}</strong>
        </div>
        <div>
          <span>Strategy</span>
          <strong>{config.strategy_path || "Built-in buy and hold"}</strong>
        </div>
        <div>
          <span>Equity curve</span>
          <strong>{config.record_equity_curve ? "Recorded" : "Skipped"}</strong>
        </div>
        <div>
          <span>Capital</span>
          <strong>
            {config.balances[0]?.amount ?? "10000"}{" "}
            {config.balances[0]?.asset ?? "USDT"}
          </strong>
        </div>
      </div>

      {(equityCurveSkipped || equityCurveDownsampled) && (
        <p className="status" aria-live="polite">
          {equityCurveSkipped && "Equity curve was not recorded for this run. "}
          {equityCurveDownsampled &&
            "Equity curve was downsampled for chart display."}
        </p>
      )}

      <section className="form-section wide">
        <div className="section-heading">
          <h3>Worker log</h3>
          <p>{workerLabel}</p>
        </div>
        <textarea
          className="log"
          readOnly
          aria-live="polite"
          aria-label="Backtest worker log"
          value={status || "No run has started in this session."}
        />
        {statusIsError && !error && (
          <p className="status-error" aria-live="assertive">
            {status}
          </p>
        )}
      </section>
    </div>
  );
}
