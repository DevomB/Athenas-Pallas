import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";
import type { ConfigDto } from "../types";
import { StatusBanner } from "./ui/StatusBanner";

interface Props {
  config: ConfigDto;
  onConfigChange: (config: ConfigDto) => void;
  onNavigate?: (tab: "config" | "run") => void;
}

function providerFromExchange(exchange: string): "yahoo" | "binance" {
  return exchange === "binance" ? "binance" : "yahoo";
}

export function FetchPanel({ config, onConfigChange, onNavigate }: Props) {
  const [provider, setProvider] = useState<"yahoo" | "binance">(
    providerFromExchange(config.exchange),
  );
  const [symbol, setSymbol] = useState(config.symbol);
  const [interval, setInterval] = useState("1d");
  const [days, setDays] = useState(30);
  const [outputPath, setOutputPath] = useState(
    config.data_path || `data/${config.symbol}_live.csv`,
  );
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState(false);
  const [fetchSuccess, setFetchSuccess] = useState(false);

  useEffect(() => {
    setSymbol(config.symbol);
    setProvider(providerFromExchange(config.exchange));
    if (config.data_path) {
      setOutputPath(config.data_path);
    }
  }, [config.symbol, config.exchange, config.data_path]);

  useEffect(() => {
    setOutputPath(`data/${symbol}_live.csv`);
    setFetchSuccess(false);
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
    setFetchSuccess(false);
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
      const exchange = provider === "binance" ? "binance" : "yahoo";
      const assetClass =
        provider === "binance"
          ? "crypto"
          : config.asset_class === "crypto"
            ? "equity"
            : config.asset_class;
      onConfigChange({
        ...config,
        data_path: path,
        symbol,
        exchange,
        asset_class: assetClass,
      });
      setStatus(`saved ${path}`);
      setFetchSuccess(true);
    } catch (e) {
      setStatus(`error: ${e}`);
    } finally {
      setBusy(false);
    }
  }

  const statusIsError = status.startsWith("error:");

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
              {(["yahoo", "binance"] as const).map((value) => (
                <label key={value}>
                  <input
                    type="radio"
                    name="provider"
                    value={value}
                    checked={provider === value}
                    onChange={() => setProvider(value)}
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

      {fetchSuccess && (
        <div className="cta-banner">
          <StatusBanner
            message={`Data saved to ${config.data_path}. Config updated with ${config.exchange}:${config.symbol}.`}
            variant="success"
          />
          <div className="row actions-row">
            <button type="button" onClick={() => onNavigate?.("config")}>
              Review config
            </button>
            <button type="button" className="secondary" onClick={() => onNavigate?.("run")}>
              Go to Run
            </button>
          </div>
        </div>
      )}

      <div className="status-panel">
        <div>
          <span>Config data path</span>
          <code>{config.data_path}</code>
        </div>
        <div>
          <span>Status</span>
          <strong
            className={statusIsError ? "status-error" : undefined}
            aria-live="polite"
          >
            {status || "Ready"}
          </strong>
        </div>
      </div>
    </div>
  );
}
