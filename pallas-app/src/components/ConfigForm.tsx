import { invoke } from "@tauri-apps/api/core";
import type { ConfigDto } from "../types";

interface Props {
  config: ConfigDto;
  onChange: (cfg: ConfigDto) => void;
}

export function ConfigForm({ config, onChange }: Props) {
  function set<K extends keyof ConfigDto>(key: K, value: ConfigDto[K]) {
    onChange({ ...config, [key]: value });
  }

  async function loadToml() {
    const path = await invoke<string | null>("pick_toml");
    if (!path) return;
    const loaded = await invoke<ConfigDto>("load_config", { path });
    onChange(loaded);
  }

  async function saveToml() {
    const path = await invoke<string | null>("pick_save_toml");
    if (!path) return;
    await invoke("save_config_toml", { path, config });
  }

  async function pickCsv() {
    const path = await invoke<string | null>("pick_csv");
    if (path) set("data_path", path);
  }

  async function pickStrategy() {
    const path = await invoke<string | null>("pick_strategy");
    if (path) set("strategy_path", path);
  }

  return (
    <div className="grid">
      <div className="row">
        <button className="secondary" onClick={loadToml}>
          Load TOML
        </button>
        <button className="secondary" onClick={saveToml}>
          Save TOML
        </button>
      </div>
      <label>
        Data path
        <div className="row">
          <input
            style={{ flex: 1 }}
            value={config.data_path}
            onChange={(e) => set("data_path", e.target.value)}
          />
          <button className="secondary" onClick={pickCsv}>
            Browse
          </button>
        </div>
      </label>
      <label>
        Data format
        <select
          value={config.data_format}
          onChange={(e) => set("data_format", e.target.value)}
        >
          <option value="auto">auto</option>
          <option value="ohlcv">ohlcv</option>
          <option value="yahoo">yahoo</option>
          <option value="fx">fx</option>
          <option value="future">future</option>
        </select>
      </label>
      <label>
        Exchange
        <input
          value={config.exchange}
          onChange={(e) => set("exchange", e.target.value)}
        />
      </label>
      <label>
        Symbol
        <input
          value={config.symbol}
          onChange={(e) => set("symbol", e.target.value)}
        />
      </label>
      <label>
        Asset class
        <select
          value={config.asset_class}
          onChange={(e) => set("asset_class", e.target.value)}
        >
          <option value="crypto">crypto</option>
          <option value="equity">equity</option>
          <option value="forex">forex</option>
          <option value="future">future</option>
        </select>
      </label>
      <div className="row">
        <label>
          Fee bps
          <input
            type="number"
            value={config.fee_bps}
            onChange={(e) => set("fee_bps", Number(e.target.value))}
          />
        </label>
        <label>
          Slippage bps
          <input
            type="number"
            value={config.slippage_bps}
            onChange={(e) => set("slippage_bps", Number(e.target.value))}
          />
        </label>
        <label>
          Half spread bps
          <input
            type="number"
            value={config.half_spread_bps}
            onChange={(e) => set("half_spread_bps", Number(e.target.value))}
          />
        </label>
      </div>
      <label>
        Periods per year
        <input
          type="number"
          value={config.periods_per_year}
          onChange={(e) => set("periods_per_year", Number(e.target.value))}
        />
      </label>
      <label>
        Strategy path
        <div className="row">
          <input
            style={{ flex: 1 }}
            value={config.strategy_path ?? ""}
            onChange={(e) => set("strategy_path", e.target.value || null)}
          />
          <button className="secondary" onClick={pickStrategy}>
            Browse
          </button>
        </div>
      </label>
      <label>
        Python executable
        <input
          value={config.python_exe}
          onChange={(e) => set("python_exe", e.target.value)}
        />
      </label>
      <label>
        <input
          type="checkbox"
          checked={config.record_equity_curve}
          onChange={(e) => set("record_equity_curve", e.target.checked)}
        />{" "}
        Record equity curve (disable for large CSV preview)
      </label>
      <label>
        Balance (asset)
        <input
          value={config.balances[0]?.asset ?? "USDT"}
          onChange={(e) =>
            set("balances", [
              { ...config.balances[0], asset: e.target.value },
            ])
          }
        />
      </label>
      <label>
        Balance (amount)
        <input
          value={config.balances[0]?.amount ?? "10000"}
          onChange={(e) =>
            set("balances", [
              { ...config.balances[0], amount: e.target.value },
            ])
          }
        />
      </label>
    </div>
  );
}
