import { useEffect, useState } from "react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
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
import { Switch } from "@/components/ui/switch";
import { ConnectionBadges } from "@/components/shared/ConnectionBadges";
import { ControlDeck } from "@/components/shared/ControlDeck";
import { FlattenConfirmDialog } from "@/components/shared/FlattenConfirmDialog";
import { FillLogTable } from "@/components/shared/FillLogTable";
import { LiveEquityChart } from "@/components/shared/LiveEquityChart";
import { OpenOrdersTable } from "@/components/shared/OpenOrdersTable";
import { PositionsBalancesCard } from "@/components/shared/PositionsBalancesCard";
import type { useTradingSession } from "@/hooks/useTradingSession";
import type { LiveSessionConfigDto } from "@/types";
import { defaultLiveConfig } from "@/types";

interface Props {
  session: ReturnType<typeof useTradingSession>;
  credentialsConfigured: boolean;
}

export function LivePage({ session, credentialsConfigured }: Props) {
  const [config, setConfig] = useState<LiveSessionConfigDto>(defaultLiveConfig());
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [flattenOpen, setFlattenOpen] = useState(false);
  const [confirmSymbol, setConfirmSymbol] = useState("");
  const isLive = session.tradingState.mode === "live";

  useEffect(() => {
    if (isLive) {
      const id = setInterval(() => session.refreshSnapshot(), 2000);
      return () => clearInterval(id);
    }
  }, [isLive, session]);

  function setField<K extends keyof LiveSessionConfigDto>(
    key: K,
    value: LiveSessionConfigDto[K],
  ) {
    setConfig((c) => ({ ...c, [key]: value }));
  }

  function requestStart() {
    if (config.use_testnet) {
      session.startLive(config).catch(() => undefined);
    } else {
      setConfirmOpen(true);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      {!credentialsConfigured && (
        <Alert variant="destructive">
          <AlertDescription>
            Binance API credentials are not configured. Add them in Settings
            before starting live trading.
          </AlertDescription>
        </Alert>
      )}
      {session.error && (
        <Alert variant="destructive">
          <AlertDescription>{session.error}</AlertDescription>
        </Alert>
      )}

      <ControlDeck
        disabled={!isLive}
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

      {isLive && (
        <div className="flex flex-wrap gap-2">
          <ConnectionBadges
            connected={session.tradingState.connected}
            connectorStatus={session.connectorStatus}
            paused={session.tradingState.paused}
            tradingEnabled={session.tradingState.trading_enabled}
          />
          <Badge variant={config.use_testnet ? "secondary" : "destructive"}>
            {config.use_testnet ? "Testnet" : "Mainnet"}
          </Badge>
          <Badge variant={credentialsConfigured ? "default" : "outline"}>
            {credentialsConfigured ? "Credentials OK" : "No credentials"}
          </Badge>
        </div>
      )}

      <div className="grid gap-4 lg:grid-cols-3">
        <Card className="lg:col-span-1">
          <CardHeader>
            <CardTitle>Live session</CardTitle>
            <CardDescription>
              Real execution - defaults to testnet for safety
            </CardDescription>
          </CardHeader>
          <CardContent className="grid gap-3">
            <div className="grid gap-2">
              <Label>Symbol</Label>
              <Input
                value={config.symbol}
                disabled={isLive}
                onChange={(e) => setField("symbol", e.target.value.toUpperCase())}
              />
            </div>
            <div className="flex items-center gap-3">
              <Switch
                id="testnet"
                checked={config.use_testnet}
                disabled={isLive}
                onCheckedChange={(v) => setField("use_testnet", v)}
              />
              <Label htmlFor="testnet">Use testnet (recommended)</Label>
            </div>
            <div className="flex gap-2">
              {!isLive ? (
                <Button
                  disabled={session.starting || !credentialsConfigured}
                  onClick={requestStart}
                >
                  {session.starting ? "Starting..." : "Start live"}
                </Button>
              ) : (
                <Button
                  variant="destructive"
                  disabled={session.stopping}
                  onClick={() => session.stopSession().catch(() => undefined)}
                >
                  Stop session
                </Button>
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
                  | L1 mark <strong>{session.snapshot.mark_price}</strong>
                </>
              )}
            </CardDescription>
          </CardHeader>
          <CardContent>
            {session.equityHistory.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                {isLive
                  ? "Waiting for equity updates..."
                  : "Start a session to see equity updates."}
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
          <CardDescription>Real-time live fills</CardDescription>
        </CardHeader>
        <CardContent>
          <FillLogTable fills={session.fills} />
        </CardContent>
      </Card>

      <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Enable mainnet trading?</AlertDialogTitle>
            <AlertDialogDescription>
              You are about to trade with real funds on Binance mainnet for{" "}
              <strong>{config.symbol}</strong>. Type the symbol to confirm.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className="px-6 pb-2">
            <Label htmlFor="confirm-symbol">Confirm symbol</Label>
            <Input
              id="confirm-symbol"
              className="mt-2"
              value={confirmSymbol}
              placeholder={config.symbol}
              onChange={(e) => setConfirmSymbol(e.target.value.toUpperCase())}
            />
          </div>
          <AlertDialogFooter>
            <AlertDialogCancel onClick={() => setConfirmSymbol("")}>
              Cancel
            </AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              disabled={confirmSymbol !== config.symbol}
              onClick={() => {
                session.startLive(config).catch(() => undefined);
                setConfirmOpen(false);
                setConfirmSymbol("");
              }}
            >
              I understand - start mainnet
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
