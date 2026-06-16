import { useCallback, useState } from "react";
import type { AppRoute } from "../types";

const ROUTE_META: Record<
  AppRoute,
  { label: string; description: string; group: "start" | "modes" | "tools" }
> = {
  "quick-start": {
    label: "Quick Start",
    description: "Guided first backtest",
    group: "start",
  },
  backtest: {
    label: "Backtest",
    description: "Historical simulation",
    group: "modes",
  },
  paper: {
    label: "Paper",
    description: "Live data, simulated fills",
    group: "modes",
  },
  live: {
    label: "Live",
    description: "Real execution",
    group: "modes",
  },
  "data-studio": {
    label: "Data Studio",
    description: "Fetch, resample, merge",
    group: "tools",
  },
  results: {
    label: "Results",
    description: "Reports and history",
    group: "tools",
  },
  settings: {
    label: "Settings",
    description: "Credentials and defaults",
    group: "tools",
  },
};

export function useAppNavigation(initial: AppRoute = "backtest") {
  const [route, setRoute] = useState<AppRoute>(initial);

  const navigate = useCallback((next: AppRoute) => {
    setRoute(next);
  }, []);

  return {
    route,
    navigate,
    meta: ROUTE_META[route],
    routes: ROUTE_META,
  };
}

export type { AppRoute };
