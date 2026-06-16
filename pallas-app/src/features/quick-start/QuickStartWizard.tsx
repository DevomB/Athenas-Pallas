import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import type { ConfigDto } from "@/types";
import type { AppRoute } from "@/types";

const STEPS = ["market", "strategy", "run"] as const;

interface Props {
  config: ConfigDto;
  onConfigChange: (c: ConfigDto) => void;
  onNavigate: (route: AppRoute) => void;
  onRun: () => Promise<void>;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}

export function QuickStartWizard({
  config,
  onConfigChange,
  onNavigate,
  onRun,
  open: controlledOpen,
  onOpenChange,
}: Props) {
  const [internalOpen, setInternalOpen] = useState(false);
  const open = controlledOpen ?? internalOpen;
  const setOpen = onOpenChange ?? setInternalOpen;
  const [step, setStep] = useState(0);
  const [busy, setBusy] = useState(false);
  const [preset, setPreset] = useState<"crypto" | "equity">("crypto");

  async function fetchPreset() {
    setBusy(true);
    try {
      const isCrypto = preset === "crypto";
      const symbol = isCrypto ? "BTCUSDT" : "AAPL";
      const provider = isCrypto ? "binance" : "yahoo";
      const output = `data/${symbol}_live.csv`;
      const path = await invoke<string>("fetch_bars", {
        req: {
          provider,
          symbol,
          interval: isCrypto ? "1d" : "1d",
          days: 90,
          output_path: output,
        },
      });
      onConfigChange({
        ...config,
        data_path: path,
        symbol,
        exchange: isCrypto ? "binance" : "yahoo",
        asset_class: isCrypto ? "crypto" : "equity",
        periods_per_year: isCrypto ? 365 : 252,
        strategy_path:
          config.strategy_path ??
          "trading/strategies/simple_sma/strategy.py",
      });
      toast.success("Data downloaded", { description: path });
      setStep(1);
    } catch (e) {
      toast.error("Fetch failed", { description: String(e) });
    } finally {
      setBusy(false);
    }
  }

  async function handleRun() {
    setBusy(true);
    try {
      await onRun();
      setOpen(false);
      onNavigate("results");
    } catch (e) {
      toast.error("Backtest failed", { description: String(e) });
    } finally {
      setBusy(false);
    }
  }

  const stepId = STEPS[step];
  const progress = ((step + 1) / STEPS.length) * 100;

  return (
    <>
      {controlledOpen === undefined && (
        <Card>
          <CardHeader>
            <CardTitle>Quick Start</CardTitle>
            <CardDescription>
              New here? Run your first backtest in three guided steps.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Button onClick={() => setOpen(true)}>Start wizard</Button>
          </CardContent>
        </Card>
      )}
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Quick Start</DialogTitle>
            <DialogDescription>
              Step {step + 1} of {STEPS.length}: {stepId}
            </DialogDescription>
          </DialogHeader>
          <Progress value={progress} className="mb-4" />
          {stepId === "market" && (
            <div className="flex flex-col gap-3">
              <p className="text-sm text-muted-foreground">
                Pick a market preset. We will download 90 days of daily bars.
              </p>
              <div className="flex gap-2">
                <Button
                  variant={preset === "crypto" ? "default" : "secondary"}
                  onClick={() => setPreset("crypto")}
                >
                  BTC (Crypto)
                </Button>
                <Button
                  variant={preset === "equity" ? "default" : "secondary"}
                  onClick={() => setPreset("equity")}
                >
                  AAPL (Stock)
                </Button>
              </div>
            </div>
          )}
          {stepId === "strategy" && (
            <div className="flex flex-col gap-2 text-sm">
              <p>
                Using strategy:{" "}
                <code className="text-xs">
                  {config.strategy_path ?? "buy & hold"}
                </code>
              </p>
              <p className="text-muted-foreground">
                Data: {config.data_path}
              </p>
              <Button
                variant="link"
                className="h-auto justify-start p-0"
                onClick={() => {
                  setOpen(false);
                  onNavigate("backtest");
                }}
              >
                Change in full config
              </Button>
            </div>
          )}
          {stepId === "run" && (
            <p className="text-sm text-muted-foreground">
              Ready to backtest {config.exchange}:{config.symbol} with{" "}
              {config.balances[0]?.amount} {config.balances[0]?.asset}.
            </p>
          )}
          <DialogFooter className="gap-2 sm:gap-0">
            {step > 0 && (
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() => setStep((s) => s - 1)}
              >
                Back
              </Button>
            )}
            {stepId === "market" && (
              <Button disabled={busy} onClick={fetchPreset}>
                {busy ? "Downloading…" : "Download & continue"}
              </Button>
            )}
            {stepId === "strategy" && (
              <Button onClick={() => setStep(2)}>Continue</Button>
            )}
            {stepId === "run" && (
              <Button disabled={busy} onClick={handleRun}>
                {busy ? "Running…" : "Run backtest"}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
