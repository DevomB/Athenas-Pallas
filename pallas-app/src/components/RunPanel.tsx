import { invoke } from "@tauri-apps/api/core";
import type { ConfigDto } from "../types";

interface Props {
  config: ConfigDto;
  running: boolean;
  status: string;
  onRunningChange: (v: boolean) => void;
  onStatus: (s: string) => void;
}

export function RunPanel({
  config,
  running,
  status,
  onRunningChange,
  onStatus,
}: Props) {
  async function start() {
    onRunningChange(true);
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
    await invoke("stop_run");
    onStatus("stop requested");
  }

  return (
    <div className="grid">
      <div className="row">
        <button disabled={running} onClick={start}>
          Start
        </button>
        <button className="secondary" disabled={!running} onClick={stop}>
          Stop
        </button>
      </div>
      <p className="status">{running ? "Running..." : "Idle"}</p>
      <p className="status">{status}</p>
      <textarea className="log" readOnly value={status} />
    </div>
  );
}
