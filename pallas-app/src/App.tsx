import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AppShell } from "@/layouts/AppShell";
import { BacktestPage } from "@/features/backtest/BacktestPage";
import { DataStudioPage } from "@/features/data-studio/DataStudioPage";
import { LivePage } from "@/features/live/LivePage";
import { PaperPage } from "@/features/paper/PaperPage";
import { QuickStartWizard } from "@/features/quick-start/QuickStartWizard";
import { ResultsPage } from "@/features/results/ResultsPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { useAppNavigation } from "@/hooks/useAppNavigation";
import { useBacktestSession } from "@/hooks/useBacktestSession";
import { useTradingSession } from "@/hooks/useTradingSession";
import { defaultConfig } from "@/types";

export default function App() {
  const { route, navigate, routes } = useAppNavigation("quick-start");
  const session = useBacktestSession(navigate);
  const trading = useTradingSession();
  const [config, setConfig] = useState(defaultConfig());
  const [credentialsConfigured, setCredentialsConfigured] = useState(false);

  useEffect(() => {
    invoke<{ api_key: string } | null>("get_credentials")
      .then((c) => setCredentialsConfigured(!!c?.api_key))
      .catch(() => setCredentialsConfigured(false));
  }, [route]);

  async function runFromWizard() {
    session.setRunning(true);
    session.clearError();
    await invoke("run_backtest", { config });
  }

  function renderRoute() {
    switch (route) {
      case "quick-start":
        return (
          <QuickStartWizard
            config={config}
            onConfigChange={setConfig}
            onNavigate={navigate}
            onRun={runFromWizard}
          />
        );
      case "backtest":
        return (
          <BacktestPage
            config={config}
            onConfigChange={setConfig}
            session={session}
            onNavigate={navigate}
          />
        );
      case "paper":
        return <PaperPage session={trading} />;
      case "live":
        return (
          <LivePage
            session={trading}
            credentialsConfigured={credentialsConfigured}
          />
        );
      case "data-studio":
        return (
          <DataStudioPage config={config} onConfigChange={setConfig} />
        );
      case "results":
        return (
          <ResultsPage
            result={session.result}
            resultSummary={session.resultSummary}
            tradingState={trading.tradingState}
            tradingSnapshot={trading.snapshot}
            onNavigate={navigate}
          />
        );
      case "settings":
        return (
          <SettingsPage config={config} onConfigChange={setConfig} />
        );
      default:
        return null;
    }
  }

  return (
    <TooltipProvider>
      <AppShell
        route={route}
        routes={routes}
        onNavigate={navigate}
        config={config}
        backtestRunning={session.running}
        tradingState={trading.tradingState}
      >
        {renderRoute()}
      </AppShell>
      <Toaster richColors position="bottom-right" />
    </TooltipProvider>
  );
}
