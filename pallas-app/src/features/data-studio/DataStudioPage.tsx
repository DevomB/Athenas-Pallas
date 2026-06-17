import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type {
  ConfigDto,
  CsvPreviewDto,
  MergeSourceDto,
} from "@/types";

interface Props {
  config: ConfigDto;
  onConfigChange: (c: ConfigDto) => void;
}

export function DataStudioPage({ config, onConfigChange }: Props) {
  const [provider, setProvider] = useState<"yahoo" | "binance">(
    config.exchange === "binance" ? "binance" : "yahoo",
  );
  const [symbol, setSymbol] = useState(config.symbol);
  const [interval, setInterval] = useState("1d");
  const [days, setDays] = useState("90");
  const [outputPath, setOutputPath] = useState(config.data_path);
  const [fetchStatus, setFetchStatus] = useState("");
  const [fetchBusy, setFetchBusy] = useState(false);

  const [resampleInput, setResampleInput] = useState(config.data_path);
  const [resampleTo, setResampleTo] = useState("1h");
  const [resampleOutput, setResampleOutput] = useState("data/resampled.csv");

  const [mergeSources, setMergeSources] = useState<MergeSourceDto[]>([
    {
      format: "ohlcv",
      exchange: config.exchange,
      symbol: config.symbol,
      path: config.data_path,
    },
  ]);
  const [mergeOutput, setMergeOutput] = useState("data/merged.csv");
  const [preview, setPreview] = useState<CsvPreviewDto | null>(null);

  const intervalOptions = useMemo(
    () =>
      provider === "binance"
        ? ["1m", "5m", "15m", "30m", "1h", "4h", "1d"]
        : ["1m", "5m", "15m", "30m", "1h", "1d", "1wk"],
    [provider],
  );

  useEffect(() => {
    const unlisten = listen<string>("fetch-progress", (e) => {
      setFetchStatus(String(e.payload));
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  async function onFetch() {
    setFetchBusy(true);
    setFetchStatus("fetching...");
    try {
      const path = await invoke<string>("fetch_bars", {
        req: {
          provider,
          symbol,
          interval,
          days: Number(days),
          output_path: outputPath,
        },
      });
      onConfigChange({
        ...config,
        data_path: path,
        symbol,
        exchange: provider === "binance" ? "binance" : "yahoo",
        asset_class: provider === "binance" ? "crypto" : config.asset_class,
      });
      setFetchStatus(`saved ${path}`);
      toast.success("Data fetched", { description: path });
      await loadPreview(path);
    } catch (e) {
      setFetchStatus(`error: ${e}`);
      toast.error(String(e));
    } finally {
      setFetchBusy(false);
    }
  }

  async function onResample() {
    try {
      const path = await invoke<string>("resample_bars", {
        req: {
          input_path: resampleInput,
          target_interval: resampleTo,
          output_path: resampleOutput,
        },
      });
      toast.success("Resampled", { description: path });
      await loadPreview(path);
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function onMerge() {
    try {
      const path = await invoke<string>("merge_bars", {
        req: { sources: mergeSources, output_path: mergeOutput },
      });
      toast.success("Merged", { description: path });
      await loadPreview(path);
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function loadPreview(path?: string) {
    const p = path ?? config.data_path;
    if (!p.trim()) return;
    try {
      const data = await invoke<CsvPreviewDto>("preview_csv", { path: p });
      setPreview(data);
    } catch {
      setPreview(null);
    }
  }

  function updateMergeSource(i: number, patch: Partial<MergeSourceDto>) {
    setMergeSources((rows) =>
      rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)),
    );
  }

  return (
    <Tabs defaultValue="fetch">
      <TabsList>
        <TabsTrigger value="fetch">Fetch</TabsTrigger>
        <TabsTrigger value="resample">Resample</TabsTrigger>
        <TabsTrigger value="merge">Merge</TabsTrigger>
        <TabsTrigger value="preview">Preview</TabsTrigger>
      </TabsList>

      <TabsContent value="fetch" className="pt-4">
        <Card>
          <CardHeader>
            <CardTitle>Download bars</CardTitle>
            <CardDescription>Yahoo or Binance OHLCV to CSV</CardDescription>
          </CardHeader>
          <CardContent className="grid gap-4 sm:grid-cols-2">
            <div className="grid gap-2">
              <Label>Provider</Label>
              <Select
                value={provider}
                onValueChange={(v) => setProvider(v as "yahoo" | "binance")}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="yahoo">Yahoo</SelectItem>
                  <SelectItem value="binance">Binance</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="grid gap-2">
              <Label>Symbol</Label>
              <Input
                value={symbol}
                onChange={(e) => setSymbol(e.target.value.toUpperCase())}
              />
            </div>
            <div className="grid gap-2">
              <Label>Interval</Label>
              <Select value={interval} onValueChange={setInterval}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {intervalOptions.map((v) => (
                    <SelectItem key={v} value={v}>
                      {v}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid gap-2">
              <Label>Lookback (days)</Label>
              <Select value={days} onValueChange={setDays}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {["7", "30", "90", "365"].map((d) => (
                    <SelectItem key={d} value={d}>
                      {d} days
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid gap-2 sm:col-span-2">
              <Label>Output path</Label>
              <Input
                value={outputPath}
                onChange={(e) => setOutputPath(e.target.value)}
              />
            </div>
            <Button disabled={fetchBusy} onClick={onFetch}>
              {fetchBusy ? "Fetching..." : "Fetch data"}
            </Button>
            {fetchStatus && (
              <p className="text-sm text-muted-foreground sm:col-span-2">
                {fetchStatus}
              </p>
            )}
          </CardContent>
        </Card>
      </TabsContent>

      <TabsContent value="resample" className="pt-4">
        <Card>
          <CardHeader>
            <CardTitle>Resample bars</CardTitle>
            <CardDescription>Aggregate to a coarser interval</CardDescription>
          </CardHeader>
          <CardContent className="grid gap-4 sm:grid-cols-2">
            <div className="grid gap-2 sm:col-span-2">
              <Label>Input CSV</Label>
              <Input
                value={resampleInput}
                onChange={(e) => setResampleInput(e.target.value)}
              />
            </div>
            <div className="grid gap-2">
              <Label>Target interval</Label>
              <Select value={resampleTo} onValueChange={setResampleTo}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {["5m", "15m", "30m", "1h", "4h", "1d"].map((v) => (
                    <SelectItem key={v} value={v}>
                      {v}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid gap-2">
              <Label>Output path</Label>
              <Input
                value={resampleOutput}
                onChange={(e) => setResampleOutput(e.target.value)}
              />
            </div>
            <Button onClick={onResample}>Resample</Button>
          </CardContent>
        </Card>
      </TabsContent>

      <TabsContent value="merge" className="pt-4">
        <Card>
          <CardHeader className="flex flex-row justify-between">
            <div>
              <CardTitle>Merge sources</CardTitle>
              <CardDescription>K-way merge by timestamp</CardDescription>
            </div>
            <Button
              variant="secondary"
              size="sm"
              onClick={() =>
                setMergeSources((r) => [
                  ...r,
                  {
                    format: "ohlcv",
                    exchange: "binance",
                    symbol: "ETHUSDT",
                    path: "",
                  },
                ])
              }
            >
              Add source
            </Button>
          </CardHeader>
          <CardContent className="flex flex-col gap-4">
            {mergeSources.map((src, i) => (
              <div
                key={i}
                className="grid gap-2 rounded-lg border p-3 sm:grid-cols-2"
              >
                <Input
                  placeholder="format"
                  value={src.format}
                  onChange={(e) =>
                    updateMergeSource(i, { format: e.target.value })
                  }
                />
                <Input
                  placeholder="exchange"
                  value={src.exchange}
                  onChange={(e) =>
                    updateMergeSource(i, { exchange: e.target.value })
                  }
                />
                <Input
                  placeholder="symbol"
                  value={src.symbol}
                  onChange={(e) =>
                    updateMergeSource(i, {
                      symbol: e.target.value.toUpperCase(),
                    })
                  }
                />
                <Input
                  placeholder="path"
                  value={src.path}
                  onChange={(e) =>
                    updateMergeSource(i, { path: e.target.value })
                  }
                />
              </div>
            ))}
            <div className="grid gap-2">
              <Label>Output path</Label>
              <Input
                value={mergeOutput}
                onChange={(e) => setMergeOutput(e.target.value)}
              />
            </div>
            <Button onClick={onMerge}>Merge</Button>
          </CardContent>
        </Card>
      </TabsContent>

      <TabsContent value="preview" className="pt-4">
        <Card>
          <CardHeader className="flex flex-row justify-between">
            <div>
              <CardTitle>CSV preview</CardTitle>
              <CardDescription>First and last rows</CardDescription>
            </div>
            <Button variant="secondary" onClick={() => loadPreview()}>
              Refresh
            </Button>
          </CardHeader>
          <CardContent>
            {!preview ? (
              <p className="text-sm text-muted-foreground">
                Load a CSV to preview rows.
              </p>
            ) : (
              <div className="flex flex-col gap-4">
                <p className="text-sm text-muted-foreground">
                  {preview.total_rows} data rows
                </p>
                <PreviewTable
                  title="Head"
                  headers={preview.headers}
                  rows={preview.head_rows}
                />
                <PreviewTable
                  title="Tail"
                  headers={preview.headers}
                  rows={preview.tail_rows}
                />
              </div>
            )}
          </CardContent>
        </Card>
      </TabsContent>
    </Tabs>
  );
}

function PreviewTable({
  title,
  headers,
  rows,
}: {
  title: string;
  headers: string[];
  rows: string[][];
}) {
  return (
    <div>
      <p className="mb-2 text-sm font-semibold">{title}</p>
      <Table>
        <TableHeader>
          <TableRow>
            {headers.map((h) => (
              <TableHead key={h}>{h}</TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((row, i) => (
            <TableRow key={i}>
              {row.map((cell, j) => (
                <TableCell key={j}>{cell}</TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
