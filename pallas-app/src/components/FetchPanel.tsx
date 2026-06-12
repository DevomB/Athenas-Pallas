import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
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
    <div className="grid">
      <label>
        Provider
        <select value={provider} onChange={(e) => setProvider(e.target.value)}>
          <option value="yahoo">Yahoo</option>
          <option value="binance">Binance</option>
        </select>
      </label>
      <label>
        Symbol
        <input value={symbol} onChange={(e) => setSymbol(e.target.value)} />
      </label>
      <label>
        Interval
        <input value={interval} onChange={(e) => setInterval(e.target.value)} />
      </label>
      <label>
        Days
        <input
          type="number"
          value={days}
          onChange={(e) => setDays(Number(e.target.value))}
        />
      </label>
      <label>
        Output path
        <input
          value={outputPath}
          onChange={(e) => setOutputPath(e.target.value)}
        />
      </label>
      <div className="row">
        <button disabled={busy} onClick={onFetch}>
          Fetch
        </button>
      </div>
      <p className="status">
        Current data path in config: <code>{config.data_path}</code>
      </p>
      <p className="status">{status}</p>
    </div>
  );
}
