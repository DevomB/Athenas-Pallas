import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { validateConfig } from "@/lib/configValidation";
import type { AppRoute, ConfigDto } from "@/types";

interface Props {
  config: ConfigDto;
  running: boolean;
  stopping: boolean;
  logLines: string[];
  error: string;
  onRunningChange: (v: boolean) => void;
  onStoppingChange: (v: boolean) => void;
  onClearError: () => void;
  onNavigate: (route: AppRoute) => void;
  equityCurveSkipped?: boolean;
  equityCurveDownsampled?: boolean;
}

export function RunPanel({
  config,
  running,
  stopping,
  logLines,
  error,
  onRunningChange,
  onStoppingChange,
  onClearError,
  onNavigate,
  equityCurveSkipped,
  equityCurveDownsampled,
}: Props) {
  const logEndRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const validationError = validateConfig(config);

  useEffect(() => {
    if (autoScroll) {
      const reduceMotion = window.matchMedia(
        "(prefers-reduced-motion: reduce)",
      ).matches;
      logEndRef.current?.scrollIntoView({
        behavior: reduceMotion ? "auto" : "smooth",
      });
    }
  }, [logLines, autoScroll]);

  async function start() {
    if (validationError) return;
    onClearError();
    onRunningChange(true);
    onStoppingChange(false);
    try {
      await invoke("run_backtest", { config });
    } catch (e) {
      onRunningChange(false);
      throw e;
    }
  }

  async function stop() {
    onStoppingChange(true);
    try {
      await invoke("stop_run");
    } catch (e) {
      onStoppingChange(false);
      throw e;
    }
  }

  return (
    <div className="flex flex-col gap-4">
      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}
      {validationError && !running && (
        <Alert variant="destructive">
          <AlertDescription>{validationError}</AlertDescription>
        </Alert>
      )}

      <Card>
        <CardHeader className="flex flex-row items-start justify-between gap-4">
          <div>
            <CardTitle>
              {config.exchange}:{config.symbol}
            </CardTitle>
            <CardDescription>
              {config.asset_class} · {config.fee_bps} bps fees ·{" "}
              {config.balances[0]?.amount ?? "10000"}{" "}
              {config.balances[0]?.asset ?? "USDT"}
            </CardDescription>
          </div>
          <div className="flex gap-2">
            <Button
              disabled={running || stopping || !!validationError}
              onClick={start}
            >
              Start backtest
            </Button>
            <Button
              variant="destructive"
              disabled={!running || stopping}
              onClick={stop}
            >
              {stopping ? "Stopping…" : "Stop"}
            </Button>
          </div>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="grid gap-2 text-sm sm:grid-cols-2">
            <div>
              <span className="text-muted-foreground">Data: </span>
              <code className="text-xs">{config.data_path}</code>
            </div>
            <div>
              <span className="text-muted-foreground">Strategy: </span>
              {config.strategy_path || "Built-in buy & hold"}
            </div>
          </div>
          <div className="flex gap-2">
            <Button
              variant="link"
              className="h-auto p-0"
              onClick={() => onNavigate("backtest")}
            >
              Edit config
            </Button>
            <Button
              variant="link"
              className="h-auto p-0"
              onClick={() => onNavigate("data-studio")}
            >
              Fetch data
            </Button>
          </div>
          {running && <Progress className="w-full" value={undefined} />}
        </CardContent>
      </Card>

      {(equityCurveSkipped || equityCurveDownsampled) && (
        <p className="text-xs text-muted-foreground">
          {equityCurveSkipped && "Equity curve was not recorded. "}
          {equityCurveDownsampled && "Equity curve will be downsampled for chart."}
        </p>
      )}

      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <div>
            <CardTitle>Worker log</CardTitle>
            <CardDescription>
              {stopping
                ? "Stopping worker…"
                : running
                  ? "Backtest in progress"
                  : "Idle"}
            </CardDescription>
          </div>
          <div className="flex items-center gap-2">
            <Switch
              id="auto-scroll"
              checked={autoScroll}
              onCheckedChange={setAutoScroll}
            />
            <Label htmlFor="auto-scroll" className="text-xs">
              Auto-scroll
            </Label>
          </div>
        </CardHeader>
        <CardContent>
          <ScrollArea className="h-48 rounded-md border bg-muted/30 p-3 font-mono text-xs">
            {logLines.length === 0 ? (
              <p className="text-muted-foreground">
                No run started in this session.
              </p>
            ) : (
              logLines.map((line, i) => (
                <div key={`${i}-${line}`}>{line}</div>
              ))
            )}
            <div ref={logEndRef} />
          </ScrollArea>
        </CardContent>
      </Card>
    </div>
  );
}
