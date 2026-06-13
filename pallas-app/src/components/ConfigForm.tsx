import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import type { ConfigDto } from "../types";
import { StatusBanner } from "./ui/StatusBanner";

interface Props {
  config: ConfigDto;
  onChange: (cfg: ConfigDto) => void;
}

const dataFormats = [
  { value: "auto", label: "Auto" },
  { value: "ohlcv", label: "OHLCV" },
  { value: "yahoo", label: "Yahoo" },
  { value: "fx", label: "FX" },
  { value: "future", label: "Futures" },
];

const assetClasses = [
  { value: "crypto", label: "Crypto" },
  { value: "equity", label: "Equity" },
  { value: "forex", label: "Forex" },
  { value: "future", label: "Future" },
];

const periodPresets = [
  { value: 252, label: "Stocks" },
  { value: 365, label: "Crypto" },
  { value: 52, label: "Weekly" },
];

function parseFeeBps(raw: string, fallback: number): number {
  const value = Number(raw);
  return Number.isFinite(value) && value >= 0 ? value : fallback;
}

export function ConfigForm({ config, onChange }: Props) {
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState("");

  function set<K extends keyof ConfigDto>(key: K, value: ConfigDto[K]) {
    onChange({ ...config, [key]: value });
  }

  function setOptional(key: keyof ConfigDto, value: string) {
    onChange({ ...config, [key]: value.trim() === "" ? null : value });
  }

  function setPrimaryBalance(key: "asset" | "amount", value: string) {
    const current = config.balances[0] ?? { asset: "USDT", amount: "10000" };
    set("balances", [{ ...current, [key]: value }]);
  }

  async function loadToml() {
    setLoading(true);
    setStatus("");
    try {
      const path = await invoke<string | null>("pick_toml");
      if (!path) return;
      const loaded = await invoke<ConfigDto>("load_config", { path });
      onChange(loaded);
      setStatus(`Loaded ${path}`);
    } catch (e) {
      setStatus(`error: ${e}`);
    } finally {
      setLoading(false);
    }
  }

  async function saveToml() {
    setSaving(true);
    setStatus("");
    try {
      const path = await invoke<string | null>("pick_save_toml");
      if (!path) return;
      await invoke("save_config_toml", { path, config });
      setStatus(`Saved ${path}`);
    } catch (e) {
      setStatus(`error: ${e}`);
    } finally {
      setSaving(false);
    }
  }

  async function pickCsv() {
    try {
      const path = await invoke<string | null>("pick_csv");
      if (path) set("data_path", path);
    } catch (e) {
      setStatus(`error: ${e}`);
    }
  }

  async function pickStrategy() {
    try {
      const path = await invoke<string | null>("pick_strategy");
      if (path) set("strategy_path", path);
    } catch (e) {
      setStatus(`error: ${e}`);
    }
  }

  const statusIsError = status.startsWith("error:");

  return (
    <div className="config-screen">
      <div className="toolbar">
        <button
          className="secondary"
          type="button"
          disabled={loading}
          onClick={loadToml}
        >
          {loading ? "Loading..." : "Load TOML"}
        </button>
        <button
          className="secondary"
          type="button"
          disabled={saving}
          onClick={saveToml}
        >
          {saving ? "Saving..." : "Save TOML"}
        </button>
      </div>

      {status && (
        <StatusBanner
          message={status}
          variant={statusIsError ? "error" : "success"}
          onDismiss={() => setStatus("")}
        />
      )}

      <div className="settings-layout">
        <section className="form-section wide">
          <div className="section-heading">
            <h3>Data</h3>
            <p>CSV source and parser mode.</p>
          </div>
          <div className="field-grid">
            <label className="wide-field">
              Data path
              <div className="input-action">
                <input
                  value={config.data_path}
                  onChange={(e) => set("data_path", e.target.value)}
                />
                <button className="secondary" type="button" onClick={pickCsv}>
                  Browse
                </button>
              </div>
            </label>
            <fieldset className="control-group wide-field">
              <legend>Data format</legend>
              <div className="segmented">
                {dataFormats.map((format) => (
                  <label key={format.value}>
                    <input
                      type="radio"
                      name="data-format"
                      value={format.value}
                      checked={config.data_format === format.value}
                      onChange={(e) => set("data_format", e.target.value)}
                    />
                    <span>{format.label}</span>
                  </label>
                ))}
              </div>
            </fieldset>
          </div>
        </section>

        <section className="form-section">
          <div className="section-heading">
            <h3>Instrument</h3>
            <p>Market identity and contract metadata.</p>
          </div>
          <div className="field-grid">
            <label>
              Exchange
              <select
                value={config.exchange}
                onChange={(e) => set("exchange", e.target.value)}
              >
                <option value="binance">Binance</option>
                <option value="yahoo">Yahoo</option>
                <option value="oanda">Oanda</option>
                <option value="cme">CME</option>
              </select>
            </label>
            <label>
              Symbol
              <input
                value={config.symbol}
                onChange={(e) => set("symbol", e.target.value.toUpperCase())}
              />
            </label>
            <fieldset className="control-group wide-field">
              <legend>Asset class</legend>
              <div className="segmented">
                {assetClasses.map((assetClass) => (
                  <label key={assetClass.value}>
                    <input
                      type="radio"
                      name="asset-class"
                      value={assetClass.value}
                      checked={config.asset_class === assetClass.value}
                      onChange={(e) => set("asset_class", e.target.value)}
                    />
                    <span>{assetClass.label}</span>
                  </label>
                ))}
              </div>
            </fieldset>
            <label>
              Lot size
              <input
                inputMode="decimal"
                placeholder="optional"
                value={config.lot_size ?? ""}
                onChange={(e) => setOptional("lot_size", e.target.value)}
              />
            </label>
            <label>
              Tick size
              <input
                inputMode="decimal"
                placeholder="optional"
                value={config.tick_size ?? ""}
                onChange={(e) => setOptional("tick_size", e.target.value)}
              />
            </label>
            <label>
              Multiplier
              <input
                inputMode="decimal"
                placeholder="futures only"
                value={config.contract_multiplier ?? ""}
                onChange={(e) =>
                  setOptional("contract_multiplier", e.target.value)
                }
              />
            </label>
            <label>
              Expiry
              <input
                placeholder="YYYY-MM or YYYY-MM-DD"
                value={config.expiry ?? ""}
                onChange={(e) => setOptional("expiry", e.target.value)}
              />
            </label>
          </div>
        </section>

        <section className="form-section">
          <div className="section-heading">
            <h3>Execution</h3>
            <p>Cost model and sampling assumptions.</p>
          </div>
          <div className="field-grid">
            <label>
              Fee bps
              <input
                type="number"
                min="0"
                value={config.fee_bps}
                onChange={(e) =>
                  set("fee_bps", parseFeeBps(e.target.value, config.fee_bps))
                }
              />
            </label>
            <label>
              Slippage bps
              <input
                type="number"
                min="0"
                value={config.slippage_bps}
                onChange={(e) =>
                  set(
                    "slippage_bps",
                    parseFeeBps(e.target.value, config.slippage_bps),
                  )
                }
              />
            </label>
            <label>
              Half spread bps
              <input
                type="number"
                min="0"
                value={config.half_spread_bps}
                onChange={(e) =>
                  set(
                    "half_spread_bps",
                    parseFeeBps(e.target.value, config.half_spread_bps),
                  )
                }
              />
            </label>
            <label>
              Periods per year
              <select
                value={String(config.periods_per_year)}
                onChange={(e) => set("periods_per_year", Number(e.target.value))}
              >
                {periodPresets.map((preset) => (
                  <option key={preset.value} value={preset.value}>
                    {preset.label} ({preset.value})
                  </option>
                ))}
                {!periodPresets.some(
                  (preset) => preset.value === config.periods_per_year,
                ) && (
                  <option value={config.periods_per_year}>
                    Custom ({config.periods_per_year})
                  </option>
                )}
              </select>
            </label>
            <label className="toggle wide-field">
              <input
                type="checkbox"
                checked={config.record_equity_curve}
                onChange={(e) => set("record_equity_curve", e.target.checked)}
              />
              <span>
                <strong>Record equity curve</strong>
                <small>Disable for very large CSV previews.</small>
              </span>
            </label>
          </div>
        </section>

        <section className="form-section">
          <div className="section-heading">
            <h3>Strategy</h3>
            <p>External strategy process and report output.</p>
          </div>
          <div className="field-grid">
            <label className="wide-field">
              Strategy path
              <div className="input-action">
                <input
                  value={config.strategy_path ?? ""}
                  onChange={(e) => set("strategy_path", e.target.value || null)}
                />
                <button
                  className="secondary"
                  type="button"
                  onClick={pickStrategy}
                >
                  Browse
                </button>
              </div>
            </label>
            <label>
              Python executable
              <select
                value={config.python_exe}
                onChange={(e) => set("python_exe", e.target.value)}
              >
                <option value="python">python</option>
                <option value="py">py</option>
                <option value="python3">python3</option>
              </select>
            </label>
            <label>
              Output JSON
              <input
                placeholder="optional"
                value={config.output_path ?? ""}
                onChange={(e) => setOptional("output_path", e.target.value)}
              />
            </label>
          </div>
        </section>

        <section className="form-section wide">
          <div className="section-heading">
            <h3>Capital</h3>
            <p>Starting balance for the simulated account.</p>
          </div>
          <div className="field-grid">
            <label>
              Balance asset
              <select
                value={config.balances[0]?.asset ?? "USDT"}
                onChange={(e) => setPrimaryBalance("asset", e.target.value)}
              >
                <option value="USDT">USDT</option>
                <option value="USD">USD</option>
                <option value="BTC">BTC</option>
                <option value="ETH">ETH</option>
                <option value="EUR">EUR</option>
              </select>
            </label>
            <label>
              Balance amount
              <input
                inputMode="decimal"
                value={config.balances[0]?.amount ?? "10000"}
                onChange={(e) => setPrimaryBalance("amount", e.target.value)}
              />
            </label>
          </div>
        </section>
      </div>
    </div>
  );
}
