import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";
import type { ConfigDto } from "../types";

interface Props {
  config: ConfigDto;
  onDataPath: (path: string) => void;
}

export function FetchPanel({ config, onDataPath }: Props) {
  const [provider, setProvider] = useState("yahoo");
  const [symbol, setSymbol] = useState("AAPL");
  const [interval, setInterval] = useState("1d");
  const [days, setDays] = useState(30);
  const [outputPath, setOutputPath] = useState(`data/${symbol}_live.csv`);
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setOutputPath(`data/${symbol}_live.csv`);
  }, [symbol]);

  const intervalOptions = useMemo(
    () =>
      provider === "binance"
        ? ["1m", "5m", "15m", "1h", "4h", "1d"]
        : ["1d", "1wk", "1mo"],
    [provider],
  );

  useEffect(() => {
    if (!intervalOptions.includes(interval)) {
      setInterval(intervalOptions[0]);
    }
  }, [interval, intervalOptions]);

  useEffect(() => {
    const unlisten = listen<string>("fetch-progress", (e) => {
      setStatus(String(e.payload));
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  async function onFetch() {
    setBusy(true);
    setStatus("fetching...");
    try {
      const path = await invoke<string>("fetch_bars", {
        req: {
          provider,
          symbol,
          interval,
          days,
          output_path: outputPath,
        },
      });
      onDataPath(path);
      setStatus(`saved ${path}`);
    } catch (e) {
      setStatus(`error: ${e}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fetch-screen">
      <section className="form-section wide">
        <div className="section-heading">
          <h3>Fetch bars</h3>
          <p>Download a CSV and attach it to the active config.</p>
        </div>
        <div className="field-grid">
          <fieldset className="control-group wide-field">
            <legend>Provider</legend>
            <div className="segmented">
              {["yahoo", "binance"].map((value) => (
                <label key={value}>
                  <input
                    type="radio"
                    name="provider"
                    value={value}
                    checked={provider === value}
                    onChange={(e) => setProvider(e.target.value)}
                  />
                  <span>{value === "yahoo" ? "Yahoo" : "Binance"}</span>
                </label>
              ))}
            </div>
          </fieldset>
          <label>
            Symbol
            <input
              value={symbol}
              onChange={(e) => setSymbol(e.target.value.toUpperCase())}
            />
          </label>
          <label>
            Interval
            <select
              value={interval}
              onChange={(e) => setInterval(e.target.value)}
            >
              {intervalOptions.map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </label>
          <label>
            Lookback
            <select
              value={String(days)}
              onChange={(e) => setDays(Number(e.target.value))}
            >
              <option value="7">7 days</option>
              <option value="30">30 days</option>
              <option value="90">90 days</option>
              <option value="365">1 year</option>
              <option value="1095">3 years</option>
            </select>
          </label>
          <label className="wide-field">
            Output path
            <input
              value={outputPath}
              onChange={(e) => setOutputPath(e.target.value)}
            />
          </label>
        </div>
        <div className="row actions-row">
          <button type="button" disabled={busy} onClick={onFetch}>
            {busy ? "Fetching..." : "Fetch data"}
          </button>
        </div>
      </section>

      <div className="status-panel">
        <div>
          <span>Config data path</span>
          <code>{config.data_path}</code>
        </div>
        <div>
          <span>Status</span>
          <strong>{status || "Ready"}</strong>
        </div>
      </div>
    </div>
  );
}
