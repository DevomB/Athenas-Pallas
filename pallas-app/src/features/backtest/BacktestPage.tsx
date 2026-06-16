import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import type { ConfigDto } from "@/types";
import type { useBacktestSession } from "@/hooks/useBacktestSession";
import type { AppRoute } from "@/types";
import { ConfigForm } from "./ConfigForm";
import { RunPanel } from "./RunPanel";

interface Props {
  config: ConfigDto;
  onConfigChange: (c: ConfigDto) => void;
  session: ReturnType<typeof useBacktestSession>;
  onNavigate: (route: AppRoute) => void;
}

export function BacktestPage({
  config,
  onConfigChange,
  session,
  onNavigate,
}: Props) {
  const runPanel = (
    <RunPanel
      config={config}
      running={session.running}
      stopping={session.stopping}
      logLines={session.logLines}
      error={session.error}
      onRunningChange={session.setRunning}
      onStoppingChange={session.setStopping}
      onClearError={session.clearError}
      onNavigate={onNavigate}
      equityCurveSkipped={session.result?.equity_curve_skipped}
      equityCurveDownsampled={session.result?.equity_curve_downsampled}
    />
  );

  return (
    <>
      <div className="hidden min-h-[640px] lg:block">
        <ResizablePanelGroup orientation="horizontal">
          <ResizablePanel defaultSize={55} minSize={35}>
            <div className="h-full overflow-auto pr-4">
              <ConfigForm config={config} onChange={onConfigChange} />
            </div>
          </ResizablePanel>
          <ResizableHandle withHandle />
          <ResizablePanel defaultSize={45} minSize={30}>
            <div className="h-full overflow-auto pl-2">{runPanel}</div>
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>

      <div className="lg:hidden">
        <Tabs defaultValue="config">
          <TabsList>
            <TabsTrigger value="config">Configure</TabsTrigger>
            <TabsTrigger value="run">Run</TabsTrigger>
          </TabsList>
          <TabsContent value="config" className="pt-4">
            <ConfigForm config={config} onChange={onConfigChange} />
          </TabsContent>
          <TabsContent value="run" className="pt-4">
            {runPanel}
          </TabsContent>
        </Tabs>
      </div>
    </>
  );
}
