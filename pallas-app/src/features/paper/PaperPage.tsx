import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Alert, AlertDescription } from "@/components/ui/alert";
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
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/components/ui/sheet";
import { FlattenConfirmDialog } from "@/components/shared/FlattenConfirmDialog";
import { ConnectionBadges } from "@/components/shared/ConnectionBadges";
import { ControlDeck } from "@/components/shared/ControlDeck";
import { FillLogTable } from "@/components/shared/FillLogTable";
import { LiveEquityChart } from "@/components/shared/LiveEquityChart";
import { OpenOrdersTable } from "@/components/shared/OpenOrdersTable";
import { PositionsBalancesCard } from "@/components/shared/PositionsBalancesCard";
import type { useTradingSession } from "@/hooks/useTradingSession";
import type { PaperSessionConfigDto, StrategyResolutionDto } from "@/types";
import { defaultPaperConfig } from "@/types";
import { validatePaperSession } from "@/lib/paperValidation";

interface Props {
  session: ReturnType<typeof useTradingSession>;
}

export function PaperPage({ session }: Props) {
  const [config, setConfig] = useState<PaperSessionConfigDto>(defaultPaperConfig());
  const [sheetOpen, setSheetOpen] = useState(false);
  const [flattenOpen, setFlattenOpen] = useState(false);
  const [strategyInfo, setStrategyInfo] =
    useState<StrategyResolutionDto | null>(null);
  const [strategyError, setStrategyError] = useState<string | null>(null);
  const isPaper = session.tradingState.mode === "paper";
  const validationError = validatePaperSession(config) ?? strategyError;

  useEffect(() => {
    if (isPaper) {
      const id = setInterval(() => session.refreshSnapshot(), 2000);
      return () => clearInterval(id);
    }
  }, [isPaper, session]);

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

  function setField<K extends keyof PaperSessionConfigDto>(
    key: K,
    value: PaperSessionConfigDto[K],
  ) {
    setConfig((c) => ({ ...c, [key]: value }));
  }

  return (
    <div className="flex flex-col gap-4">
      {session.error && (
        <Alert variant="destructive">
          <AlertDescription>{session.error}</AlertDescription>
        </Alert>
      )}

      {validationError && !isPaper && (
        <Alert variant="destructive">
          <AlertDescription>{validationError}</AlertDescription>
        </Alert>
      )}

      <ControlDeck
        disabled={!isPaper}
        paused={session.tradingState.paused}
        tradingEnabled={session.tradingState.trading_enabled}
        onPause={() => session.control("trading_pause")}
        onResume={() => session.control("trading_resume")}
        onTradingEnable={() => session.control("trading_enable")}
        onTradingDisable={() => session.control("trading_disable")}
        onCancelAll={() => session.control("cancel_all_orders")}
        onFlatten={() => setFlattenOpen(true)}
      />

      <FlattenConfirmDialog
        open={flattenOpen}
        onOpenChange={setFlattenOpen}
        onConfirm={() => {
          session.control("flatten_all").catch(() => undefined);
          setFlattenOpen(false);
        }}
      />

      <div className="flex flex-wrap gap-2">
        <Badge variant={isPaper ? "default" : "outline"}>
          {isPaper ? "Paper active" : "Idle"}
        </Badge>
        {isPaper && (
          <ConnectionBadges
            connected={session.tradingState.connected}
            connectorStatus={session.connectorStatus}
            paused={session.tradingState.paused}
            tradingEnabled={session.tradingState.trading_enabled}
          />
        )}
      </div>

      <div className="grid gap-4 lg:grid-cols-3">
        <Card className="lg:col-span-1">
          <CardHeader className="flex flex-row items-start justify-between">
            <div>
              <CardTitle>Session</CardTitle>
              <CardDescription>Paper trading with live market data</CardDescription>
            </div>
            {isPaper && (
              <Sheet open={sheetOpen} onOpenChange={setSheetOpen}>
                <SheetTrigger asChild>
                  <Button variant="outline" size="sm">
                    Edit
                  </Button>
                </SheetTrigger>
                <SheetContent>
                  <SheetHeader>
                    <SheetTitle>Session config</SheetTitle>
                    <SheetDescription>
                      View-only while session is active
                    </SheetDescription>
                  </SheetHeader>
                  <div className="mt-4 grid gap-2 text-sm">
                    <p>
                      <span className="text-muted-foreground">Symbol: </span>
                      {config.symbol}
                    </p>
                    <p>
                      <span className="text-muted-foreground">Balance: </span>
                      {config.starting_balance_amount}{" "}
                      {config.starting_balance_asset}
                    </p>
                    <p>
                      <span className="text-muted-foreground">Strategy: </span>
                      {config.strategy_path || "Hold (built-in)"}
                    </p>
                  </div>
                </SheetContent>
              </Sheet>
            )}
          </CardHeader>
          <CardContent className="grid gap-3">
            <div className="grid gap-2">
              <Label>Symbol</Label>
              <Input
                value={config.symbol}
                disabled={isPaper}
                onChange={(e) =>
                  setField("symbol", e.target.value.toUpperCase())
                }
              />
            </div>
            <div className="grid gap-2">
              <Label>Starting balance</Label>
              <div className="flex gap-2">
                <Input
                  value={config.starting_balance_amount}
                  disabled={isPaper}
                  onChange={(e) =>
                    setField("starting_balance_amount", e.target.value)
                  }
                />
                <Select
                  value={config.starting_balance_asset}
                  disabled={isPaper}
                  onValueChange={(v) => setField("starting_balance_asset", v)}
                >
                  <SelectTrigger className="w-24">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="USDT">USDT</SelectItem>
                    <SelectItem value="USD">USD</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
            <div className="grid gap-2">
              <Label>Strategy</Label>
              <div className="flex flex-wrap gap-2">
                <Input
                  className="min-w-56 flex-1"
                  value={config.strategy_path ?? ""}
                  disabled={isPaper}
                  placeholder="simple_sma, folder, or file"
                  onChange={(e) =>
                    setField("strategy_path", e.target.value || null)
                  }
                />
                <Button
                  variant="secondary"
                  disabled={isPaper}
                  onClick={async () => {
                    const path = await invoke<string | null>("pick_strategy_dir");
                    if (path) setField("strategy_path", path);
                  }}
                >
                  Folder
                </Button>
                <Button
                  variant="secondary"
                  disabled={isPaper}
                  onClick={async () => {
                    const path = await invoke<string | null>("pick_strategy");
                    if (path) setField("strategy_path", path);
                  }}
                >
                  File
                </Button>
              </div>
              {strategyInfo && (
                <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                  <Badge variant="secondary">{strategyInfo.kind}</Badge>
                  <span className="break-all">{strategyInfo.path}</span>
                </div>
              )}
            </div>
            <div className="flex gap-2">
              {!isPaper ? (
                <Button
                  disabled={session.starting || !!validationError}
                  onClick={() =>
                    session.startPaper(config).catch(() => undefined)
                  }
                >
                  {session.starting ? "Starting..." : "Start paper"}
                </Button>
              ) : (
                <Button
                  variant="destructive"
                  disabled={session.stopping}
                  onClick={() => session.stopSession().catch(() => undefined)}
                >
                  {session.stopping ? "Stopping..." : "Stop session"}
                </Button>
              )}
            </div>
            <div className="flex flex-wrap gap-1">
              <Badge variant={isPaper ? "default" : "outline"}>
                {isPaper ? "Active" : "Idle"}
              </Badge>
              {session.snapshot && (
                <Badge variant="secondary">
                  Equity: {session.snapshot.equity}
                </Badge>
              )}
            </div>
          </CardContent>
        </Card>

        <Card className="lg:col-span-2">
          <CardHeader>
            <CardTitle>Live equity</CardTitle>
            <CardDescription>
              Session equity over time
              {session.snapshot?.mark_price && (
                <>
                  {" "}
                  | L1 mark{" "}
                  <strong>{session.snapshot.mark_price}</strong>
                </>
              )}
            </CardDescription>
          </CardHeader>
          <CardContent>
            {session.equityHistory.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                Start a session to see equity updates.
              </p>
            ) : (
              <LiveEquityChart points={session.equityHistory} />
            )}
          </CardContent>
        </Card>

        <Card className="lg:col-span-1">
          <CardHeader>
            <CardTitle>Positions & balances</CardTitle>
          </CardHeader>
          <CardContent>
            <PositionsBalancesCard snapshot={session.snapshot} />
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Open orders</CardTitle>
        </CardHeader>
        <CardContent>
          <OpenOrdersTable orders={session.openOrders} />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Fill log</CardTitle>
          <CardDescription>Real-time paper fills</CardDescription>
        </CardHeader>
        <CardContent>
          <FillLogTable fills={session.fills} />
        </CardContent>
      </Card>
    </div>
  );
}
