import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { InputGroup } from "@/components/ui/input-group";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Alert, AlertDescription } from "@/components/ui/alert";
import type {
  ConfigDto,
  ExtraInstrumentDto,
  StrategyResolutionDto,
} from "@/types";
import { validateConfig } from "@/lib/configValidation";

interface Props {
  config: ConfigDto;
  onChange: (cfg: ConfigDto) => void;
}

const FEE_PRESETS = [
  { label: "Low (5 bps)", fee: 5, slip: 2, spread: 2 },
  { label: "Medium (10 bps)", fee: 10, slip: 5, spread: 5 },
  { label: "High (25 bps)", fee: 25, slip: 10, spread: 10 },
];

const ASSET_CLASSES = [
  "crypto",
  "equity",
  "forex",
  "future",
  "option",
  "perpetual",
  "bond",
  "hybrid",
];

function parseFeeBps(raw: string, fallback: number): number {
  const value = Number(raw);
  return Number.isFinite(value) && value >= 0 ? value : fallback;
}

export function ConfigForm({ config, onChange }: Props) {
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [strategyInfo, setStrategyInfo] =
    useState<StrategyResolutionDto | null>(null);
  const [strategyError, setStrategyError] = useState<string | null>(null);
  const validationError = validateConfig(config);

  useEffect(() => {
    const path = config.strategy_path?.trim();
    if (!path) {
      setStrategyInfo(null);
      setStrategyError(null);
      return;
    }
    let cancelled = false;
    invoke<StrategyResolutionDto>("detect_strategy", { path })
      .then((info) => {
        if (!cancelled) {
          setStrategyInfo(info);
          setStrategyError(null);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setStrategyInfo(null);
          setStrategyError(String(e));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [config.strategy_path]);

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

  function applyFeePreset(fee: number, slip: number, spread: number) {
    onChange({
      ...config,
      fee_bps: fee,
      slippage_bps: slip,
      half_spread_bps: spread,
    });
  }

  async function loadToml() {
    setLoading(true);
    try {
      const path = await invoke<string | null>("pick_toml");
      if (!path) return;
      const loaded = await invoke<ConfigDto>("load_config", { path });
      onChange(loaded);
      toast.success("Config loaded", { description: path });
    } catch (e) {
      toast.error("Failed to load config", { description: String(e) });
    } finally {
      setLoading(false);
    }
  }

  async function saveToml() {
    setSaving(true);
    try {
      const path = await invoke<string | null>("pick_save_toml");
      if (!path) return;
      await invoke("save_config_toml", { path, config });
      toast.success("Config saved", { description: path });
    } catch (e) {
      toast.error("Failed to save config", { description: String(e) });
    } finally {
      setSaving(false);
    }
  }

  async function pickCsv() {
    try {
      const path = await invoke<string | null>("pick_csv");
      if (path) set("data_path", path);
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function pickStrategy() {
    try {
      const path = await invoke<string | null>("pick_strategy");
      if (path) set("strategy_path", path);
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function pickStrategyDir() {
    try {
      const path = await invoke<string | null>("pick_strategy_dir");
      if (path) set("strategy_path", path);
    } catch (e) {
      toast.error(String(e));
    }
  }

  function updateExtra(index: number, patch: Partial<ExtraInstrumentDto>) {
    const extras = [...(config.extra_instruments ?? [])];
    extras[index] = { ...extras[index], ...patch };
    set("extra_instruments", extras);
  }

  function addExtra() {
    set("extra_instruments", [
      ...(config.extra_instruments ?? []),
      {
        exchange: "yahoo",
        symbol: "AAPL",
        asset_class: "equity",
        data_path: "",
        data_format: "yahoo",
      },
    ]);
  }

  function removeExtra(index: number) {
    set(
      "extra_instruments",
      (config.extra_instruments ?? []).filter((_, i) => i !== index),
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap gap-2">
        <Button variant="secondary" disabled={loading} onClick={loadToml}>
          {loading ? "Loading..." : "Load TOML"}
        </Button>
        <Button variant="secondary" disabled={saving} onClick={saveToml}>
          {saving ? "Saving..." : "Save TOML"}
        </Button>
      </div>

      {validationError && (
        <Alert variant="destructive">
          <AlertDescription>{validationError}</AlertDescription>
        </Alert>
      )}

      <Tabs defaultValue="simple">
        <TabsList>
          <TabsTrigger value="simple">Simple</TabsTrigger>
          <TabsTrigger value="advanced">Advanced</TabsTrigger>
        </TabsList>

        <TabsContent value="simple" className="flex flex-col gap-4 pt-4">
          <Card>
            <CardHeader>
              <CardTitle>Market</CardTitle>
              <CardDescription>What you want to backtest</CardDescription>
            </CardHeader>
            <CardContent className="grid gap-4 sm:grid-cols-2">
              <div className="grid gap-2">
                <div className="flex items-center gap-1">
                  <Label htmlFor="symbol">Symbol</Label>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <span className="cursor-help text-xs text-muted-foreground">
                        ?
                      </span>
                    </TooltipTrigger>
                    <TooltipContent>
                      Ticker to backtest, e.g. BTCUSDT or AAPL
                    </TooltipContent>
                  </Tooltip>
                </div>
                <Input
                  id="symbol"
                  value={config.symbol}
                  aria-invalid={!config.symbol.trim()}
                  data-invalid={!config.symbol.trim() || undefined}
                  onChange={(e) => set("symbol", e.target.value.toUpperCase())}
                />
              </div>
              <div className="grid gap-2">
                <Label htmlFor="exchange">Exchange</Label>
                <Select
                  value={config.exchange}
                  onValueChange={(v) => set("exchange", v)}
                >
                  <SelectTrigger id="exchange">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="binance">Binance</SelectItem>
                    <SelectItem value="yahoo">Yahoo</SelectItem>
                    <SelectItem value="oanda">Oanda</SelectItem>
                    <SelectItem value="cme">CME</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="grid gap-2 sm:col-span-2">
                <div className="flex items-center gap-1">
                  <Label htmlFor="data-path">Data file (CSV)</Label>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <span className="cursor-help text-xs text-muted-foreground">
                        ?
                      </span>
                    </TooltipTrigger>
                    <TooltipContent>
                      Historical OHLCV CSV - fetch one in Data Studio if needed
                    </TooltipContent>
                  </Tooltip>
                </div>
                <InputGroup>
                  <Input
                    id="data-path"
                    value={config.data_path}
                    aria-invalid={!config.data_path.trim()}
                    data-invalid={!config.data_path.trim() || undefined}
                    onChange={(e) => set("data_path", e.target.value)}
                  />
                  <Button variant="secondary" onClick={pickCsv}>
                    Browse
                  </Button>
                </InputGroup>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Strategy</CardTitle>
              <CardDescription>
                Folder, name, Python file, or compiled binary
              </CardDescription>
            </CardHeader>
            <CardContent className="grid gap-4">
              <div className="grid gap-2">
                <Label htmlFor="strategy">Strategy</Label>
                <div className="flex flex-wrap gap-2">
                  <Input
                    id="strategy"
                    className="min-w-64 flex-1"
                    value={config.strategy_path ?? ""}
                    onChange={(e) =>
                      set("strategy_path", e.target.value || null)
                    }
                    placeholder="simple_sma, trading/simple_sma, or a file path"
                  />
                  <Button variant="secondary" onClick={pickStrategyDir}>
                    Folder
                  </Button>
                  <Button variant="secondary" onClick={pickStrategy}>
                    File
                  </Button>
                </div>
                {strategyInfo && (
                  <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <Badge variant="secondary">{strategyInfo.kind}</Badge>
                    <span className="break-all">{strategyInfo.path}</span>
                  </div>
                )}
                {strategyError && (
                  <p className="text-xs text-destructive">{strategyError}</p>
                )}
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Capital & costs</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-4 sm:grid-cols-2">
              <div className="grid gap-2">
                <Label>Starting balance</Label>
                <div className="flex gap-2">
                  <Input
                    inputMode="decimal"
                    value={config.balances[0]?.amount ?? "10000"}
                    onChange={(e) =>
                      setPrimaryBalance("amount", e.target.value)
                    }
                  />
                  <Select
                    value={config.balances[0]?.asset ?? "USDT"}
                    onValueChange={(v) => setPrimaryBalance("asset", v)}
                  >
                    <SelectTrigger className="w-28">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {["USDT", "USD", "BTC", "ETH", "EUR"].map((a) => (
                        <SelectItem key={a} value={a}>
                          {a}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </div>
              <div className="grid gap-2">
                <Label>Fee preset</Label>
                <Select
                  onValueChange={(v) => {
                    const p = FEE_PRESETS.find((x) => x.label === v);
                    if (p) applyFeePreset(p.fee, p.slip, p.spread);
                  }}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Choose preset" />
                  </SelectTrigger>
                  <SelectContent>
                    {FEE_PRESETS.map((p) => (
                      <SelectItem key={p.label} value={p.label}>
                        {p.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="advanced" className="flex flex-col gap-4 pt-4">
          <Card>
            <CardHeader>
              <CardTitle>Execution model</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-4 sm:grid-cols-2">
              <div className="grid gap-2">
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Label>Fee (bps)</Label>
                  </TooltipTrigger>
                  <TooltipContent>
                    Trading fee in basis points (1 bps = 0.01%)
                  </TooltipContent>
                </Tooltip>
                <Input
                  type="number"
                  min={0}
                  value={config.fee_bps}
                  onChange={(e) =>
                    set("fee_bps", parseFeeBps(e.target.value, config.fee_bps))
                  }
                />
              </div>
              <div className="grid gap-2">
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Label>Slippage (bps)</Label>
                  </TooltipTrigger>
                  <TooltipContent>
                    Models worse fill prices than mid
                  </TooltipContent>
                </Tooltip>
                <Input
                  type="number"
                  min={0}
                  value={config.slippage_bps}
                  onChange={(e) =>
                    set(
                      "slippage_bps",
                      parseFeeBps(e.target.value, config.slippage_bps),
                    )
                  }
                />
              </div>
              <div className="grid gap-2">
                <Label>Half spread (bps)</Label>
                <Input
                  type="number"
                  min={0}
                  value={config.half_spread_bps}
                  onChange={(e) =>
                    set(
                      "half_spread_bps",
                      parseFeeBps(e.target.value, config.half_spread_bps),
                    )
                  }
                />
              </div>
              <div className="grid gap-2">
                <Label>Periods per year</Label>
                <Input
                  type="number"
                  value={config.periods_per_year}
                  onChange={(e) =>
                    set("periods_per_year", Number(e.target.value))
                  }
                />
              </div>
              <div className="grid gap-2">
                <Label>Bar interval</Label>
                <Input
                  placeholder="e.g. 30m, 1d"
                  value={config.bar_interval ?? ""}
                  onChange={(e) => setOptional("bar_interval", e.target.value)}
                />
              </div>
              <div className="grid gap-2">
                <Label>Session filter</Label>
                <Select
                  value={config.session_filter ?? "none"}
                  onValueChange={(v) =>
                    set("session_filter", v === "none" ? null : v)
                  }
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="none">None</SelectItem>
                    <SelectItem value="equity_rth">Equity RTH</SelectItem>
                    <SelectItem value="forex_245">Forex 24/5</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="grid gap-2">
                <Label>Risk-free rate (annual)</Label>
                <Input
                  type="number"
                  step="0.01"
                  value={config.risk_free_annual ?? 0}
                  onChange={(e) =>
                    set("risk_free_annual", Number(e.target.value))
                  }
                />
              </div>
              <div className="flex items-center gap-3">
                <Switch
                  id="auto-periods"
                  checked={config.auto_periods_per_year ?? true}
                  onCheckedChange={(v) => set("auto_periods_per_year", v)}
                />
                <Label htmlFor="auto-periods">Auto periods per year</Label>
              </div>
              <div className="flex items-center gap-3">
                <Switch
                  id="record-equity"
                  checked={config.record_equity_curve}
                  onCheckedChange={(v) => set("record_equity_curve", v)}
                />
                <Label htmlFor="record-equity">Record equity curve</Label>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Instrument metadata</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-4 sm:grid-cols-2">
              <div className="grid gap-2">
                <Label>Asset class</Label>
                <Select
                  value={config.asset_class}
                  onValueChange={(v) => set("asset_class", v)}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {ASSET_CLASSES.map((ac) => (
                      <SelectItem key={ac} value={ac}>
                        {ac}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="grid gap-2">
                <Label>Data format</Label>
                <Select
                  value={config.data_format}
                  onValueChange={(v) => set("data_format", v)}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {["auto", "ohlcv", "yahoo", "fx", "future"].map((f) => (
                      <SelectItem key={f} value={f}>
                        {f}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              {(
                [
                  ["lot_size", "Lot size"],
                  ["tick_size", "Tick size"],
                  ["contract_multiplier", "Multiplier"],
                  ["expiry", "Expiry"],
                  ["max_position_abs", "Max position"],
                  ["max_daily_loss_quote", "Max daily loss"],
                  ["margin_initial_rate", "Margin rate"],
                ] as const
              ).map(([key, label]) => (
                <div key={key} className="grid gap-2">
                  <Label>{label}</Label>
                  <Input
                    value={(config[key] as string | null | undefined) ?? ""}
                    onChange={(e) => setOptional(key, e.target.value)}
                  />
                </div>
              ))}
              <div className="grid gap-2">
                <Label>Python executable</Label>
                <Select
                  value={config.python_exe}
                  onValueChange={(v) => set("python_exe", v)}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {["python", "py", "python3"].map((p) => (
                      <SelectItem key={p} value={p}>
                        {p}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="grid gap-2 sm:col-span-2">
                <Label>Output JSON path</Label>
                <Input
                  value={config.output_path ?? ""}
                  onChange={(e) => setOptional("output_path", e.target.value)}
                />
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <div>
                <CardTitle>Extra instruments</CardTitle>
                <CardDescription>Multi-instrument replay</CardDescription>
              </div>
              <Button variant="secondary" size="sm" onClick={addExtra}>
                Add instrument
              </Button>
            </CardHeader>
            <CardContent className="flex flex-col gap-4">
              {(config.extra_instruments ?? []).length === 0 && (
                <p className="text-sm text-muted-foreground">
                  No extra instruments. Add rows for merged multi-symbol
                  backtests.
                </p>
              )}
              {(config.extra_instruments ?? []).map((row, i) => (
                <div
                  key={i}
                  className="grid gap-3 rounded-lg border p-3 sm:grid-cols-2"
                >
                  <Input
                    placeholder="Exchange"
                    value={row.exchange}
                    onChange={(e) =>
                      updateExtra(i, { exchange: e.target.value })
                    }
                  />
                  <Input
                    placeholder="Symbol"
                    value={row.symbol}
                    onChange={(e) =>
                      updateExtra(i, { symbol: e.target.value.toUpperCase() })
                    }
                  />
                  <Input
                    placeholder="Data path"
                    className="sm:col-span-2"
                    value={row.data_path ?? ""}
                    onChange={(e) =>
                      updateExtra(i, { data_path: e.target.value })
                    }
                  />
                  <Button
                    variant="destructive"
                    size="sm"
                    onClick={() => removeExtra(i)}
                  >
                    Remove
                  </Button>
                </div>
              ))}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
