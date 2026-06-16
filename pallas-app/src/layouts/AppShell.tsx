import {
  Activity,
  BarChart3,
  Database,
  FlaskConical,
  PlayCircle,
  Radio,
  Settings,
  Sparkles,
  Zap,
} from "lucide-react";
import type { ReactNode } from "react";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
  SidebarSeparator,
} from "@/components/ui/sidebar";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import type { AppRoute } from "@/types";
import type { ConfigDto, TradingStateDto } from "@/types";

const NAV_START: Array<{ id: AppRoute; icon: typeof Sparkles }> = [
  { id: "quick-start", icon: Sparkles },
];

const NAV_MODES: Array<{ id: AppRoute; icon: typeof BarChart3 }> = [
  { id: "backtest", icon: BarChart3 },
  { id: "paper", icon: FlaskConical },
  { id: "live", icon: Radio },
];

const NAV_TOOLS: Array<{ id: AppRoute; icon: typeof Database }> = [
  { id: "data-studio", icon: Database },
  { id: "results", icon: Activity },
  { id: "settings", icon: Settings },
];

interface Props {
  route: AppRoute;
  routes: Record<AppRoute, { label: string; description: string }>;
  onNavigate: (route: AppRoute) => void;
  config: ConfigDto;
  backtestRunning: boolean;
  tradingState: TradingStateDto;
  children: ReactNode;
}

export function AppShell({
  route,
  routes,
  onNavigate,
  config,
  backtestRunning,
  tradingState,
  children,
}: Props) {
  const sessionLabel =
    tradingState.mode !== "idle"
      ? tradingState.mode === "paper"
        ? "Paper"
        : "Live"
      : backtestRunning
        ? "Backtest"
        : "Idle";

  return (
    <SidebarProvider>
      <Sidebar collapsible="icon" variant="inset">
        <SidebarHeader className="gap-3 p-4">
          <div className="flex items-center gap-3">
            <div className="flex size-10 items-center justify-center rounded-lg bg-primary font-black text-primary-foreground">
              P
            </div>
            <div className="min-w-0 group-data-[collapsible=icon]:hidden">
              <p className="truncate text-sm font-bold">Pallas</p>
              <p className="truncate text-xs text-muted-foreground">
                Trading workbench
              </p>
            </div>
          </div>
        </SidebarHeader>
        <SidebarContent>
          <SidebarGroup>
            <SidebarGroupLabel>Get started</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {NAV_START.map(({ id, icon: Icon }) => (
                  <SidebarMenuItem key={id}>
                    <SidebarMenuButton
                      isActive={route === id}
                      onClick={() => onNavigate(id)}
                      tooltip={routes[id].label}
                    >
                      <Icon />
                      <span>{routes[id].label}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
          <SidebarSeparator />
          <SidebarGroup>
            <SidebarGroupLabel>Modes</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {NAV_MODES.map(({ id, icon: Icon }) => (
                  <SidebarMenuItem key={id}>
                    <SidebarMenuButton
                      isActive={route === id}
                      onClick={() => onNavigate(id)}
                      tooltip={routes[id].label}
                    >
                      <Icon />
                      <span>{routes[id].label}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
          <SidebarSeparator />
          <SidebarGroup>
            <SidebarGroupLabel>Tools</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {NAV_TOOLS.map(({ id, icon: Icon }) => (
                  <SidebarMenuItem key={id}>
                    <SidebarMenuButton
                      isActive={route === id}
                      onClick={() => onNavigate(id)}
                      tooltip={routes[id].label}
                    >
                      <Icon />
                      <span>{routes[id].label}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>
        <SidebarFooter className="p-3 group-data-[collapsible=icon]:hidden">
          <div className="rounded-lg border bg-card p-3 text-xs">
            <div className="mb-2 flex items-center gap-2">
              <span
                className={`size-2 rounded-full ${
                  backtestRunning || tradingState.mode !== "idle"
                    ? "bg-emerald-500"
                    : "bg-muted-foreground"
                }`}
              />
              <span className="font-semibold">{sessionLabel}</span>
            </div>
            <p className="truncate text-muted-foreground">
              {config.exchange}:{config.symbol}
            </p>
            {tradingState.mode !== "idle" && (
              <div className="mt-2 flex flex-wrap gap-1">
                {tradingState.paused && (
                  <Badge variant="secondary">Paused</Badge>
                )}
                {!tradingState.trading_enabled && (
                  <Badge variant="outline">Trading off</Badge>
                )}
                <Badge
                  variant={tradingState.connected ? "default" : "destructive"}
                >
                  {tradingState.connected ? "Connected" : "Disconnected"}
                </Badge>
              </div>
            )}
          </div>
        </SidebarFooter>
        <SidebarRail />
      </Sidebar>
      <SidebarInset>
        <header className="flex h-14 shrink-0 items-center gap-2 border-b px-6">
          <div className="flex min-w-0 flex-1 flex-col">
            <p className="text-xs font-semibold uppercase tracking-wide text-primary">
              {routes[route].description}
            </p>
            <h1 className="truncate text-lg font-bold">{routes[route].label}</h1>
          </div>
          <div className="hidden items-center gap-2 md:flex">
            <Badge variant="outline">{config.asset_class}</Badge>
            <Badge variant="outline">{config.data_format}</Badge>
            {backtestRunning && (
              <Badge className="gap-1">
                <PlayCircle className="size-3" />
                Running
              </Badge>
            )}
            {tradingState.mode !== "idle" && (
              <Badge className="gap-1">
                <Zap className="size-3" />
                {tradingState.mode}
              </Badge>
            )}
          </div>
        </header>
        <div className="flex flex-1 flex-col gap-4 p-6">{children}</div>
        <footer className="border-t px-6 py-2">
          <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            <span>Session: {sessionLabel}</span>
            <Separator orientation="vertical" className="h-3" />
            <span>
              {config.exchange}:{config.symbol}
            </span>
            {tradingState.mode !== "idle" && (
              <>
                <Separator orientation="vertical" className="h-3" />
                <span>
                  {tradingState.connected ? "Market connected" : "Reconnecting"}
                </span>
              </>
            )}
          </div>
        </footer>
      </SidebarInset>
    </SidebarProvider>
  );
}
