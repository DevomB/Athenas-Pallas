import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { mapUserError } from "../lib/errorMessages";
import type {
  ConnectorStatusDto,
  FillEventDto,
  LiveEquityPoint,
  LiveSessionConfigDto,
  OpenOrderDto,
  PaperSessionConfigDto,
  PositionsSnapshotDto,
  TradingStateDto,
} from "../types";

const DEFAULT_TRADING_STATE: TradingStateDto = {
  mode: "idle",
  instrument: "",
  paused: false,
  trading_enabled: true,
  connected: false,
};

export function useTradingSession() {
  const [tradingState, setTradingState] =
    useState<TradingStateDto>(DEFAULT_TRADING_STATE);
  const [snapshot, setSnapshot] = useState<PositionsSnapshotDto | null>(null);
  const [fills, setFills] = useState<FillEventDto[]>([]);
  const [equityHistory, setEquityHistory] = useState<LiveEquityPoint[]>([]);
  const [openOrders, setOpenOrders] = useState<OpenOrderDto[]>([]);
  const [connectorStatus, setConnectorStatus] =
    useState<ConnectorStatusDto["status"]>("disconnected");
  const [error, setError] = useState("");
  const [starting, setStarting] = useState(false);
  const [stopping, setStopping] = useState(false);

  useEffect(() => {
    const unsubs: Array<() => void> = [];

    listen<TradingStateDto>("trading-state-changed", (e) => {
      setTradingState(e.payload);
    }).then((fn) => unsubs.push(fn));

    listen<PositionsSnapshotDto>("equity-tick", (e) => {
      setSnapshot(e.payload);
      const equity = parseFloat(e.payload.equity);
      if (!Number.isNaN(equity)) {
        setEquityHistory((prev) => [
          ...prev.slice(-499),
          { time: Math.floor(Date.now() / 1000), equity },
        ]);
      }
    }).then((fn) => unsubs.push(fn));

    listen<FillEventDto>("fill", (e) => {
      setFills((prev) => [...prev.slice(-499), e.payload]);
    }).then((fn) => unsubs.push(fn));

    listen<OpenOrderDto[]>("order-update", (e) => {
      setOpenOrders(e.payload);
    }).then((fn) => unsubs.push(fn));

    listen<ConnectorStatusDto>("connector-status", (e) => {
      setConnectorStatus(e.payload.status);
      setTradingState((prev) => ({
        ...prev,
        connected: e.payload.status === "connected",
      }));
    }).then((fn) => unsubs.push(fn));

    listen<string>("session-error", (e) => {
      setError(mapUserError(e.payload));
    }).then((fn) => unsubs.push(fn));

    listen("trading-session-started", () => {
      setStarting(false);
      setError("");
    }).then((fn) => unsubs.push(fn));

    listen("trading-session-stopped", () => {
      setStopping(false);
      setTradingState(DEFAULT_TRADING_STATE);
      setSnapshot(null);
      setEquityHistory([]);
      setOpenOrders([]);
      setConnectorStatus("disconnected");
    }).then((fn) => unsubs.push(fn));

    return () => {
      unsubs.forEach((fn) => fn());
    };
  }, []);

  const refreshOpenOrders = useCallback(async () => {
    try {
      const orders = await invoke<OpenOrderDto[]>("list_open_orders");
      setOpenOrders(orders);
    } catch {
      setOpenOrders([]);
    }
  }, []);

  const refreshSnapshot = useCallback(async () => {
    try {
      const data = await invoke<PositionsSnapshotDto>("get_positions_snapshot");
      setSnapshot(data);
      setTradingState((prev) => ({
        ...prev,
        paused: data.paused,
        trading_enabled: data.trading_enabled,
        connected: data.connected,
      }));
      const equity = parseFloat(data.equity);
      if (!Number.isNaN(equity)) {
        setEquityHistory((prev) => [
          ...prev.slice(-499),
          { time: Math.floor(Date.now() / 1000), equity },
        ]);
      }
      await refreshOpenOrders();
    } catch {
      // Session may be idle
    }
  }, [refreshOpenOrders]);

  const startPaper = useCallback(async (config: PaperSessionConfigDto) => {
    setStarting(true);
    setError("");
    setFills([]);
    setEquityHistory([]);
    setOpenOrders([]);
    try {
      await invoke("start_paper_session", { config });
    } catch (e) {
      setStarting(false);
      setError(mapUserError(e));
      throw e;
    }
  }, []);

  const startLive = useCallback(async (config: LiveSessionConfigDto) => {
    setStarting(true);
    setError("");
    setFills([]);
    setEquityHistory([]);
    setOpenOrders([]);
    try {
      await invoke("start_live_session", { config });
    } catch (e) {
      setStarting(false);
      setError(mapUserError(e));
      throw e;
    }
  }, []);

  const stopSession = useCallback(async () => {
    setStopping(true);
    try {
      await invoke("stop_trading_session");
    } catch (e) {
      setStopping(false);
      setError(mapUserError(e));
      throw e;
    }
  }, []);

  const control = useCallback(async (cmd: string) => {
    try {
      await invoke(cmd);
      await refreshSnapshot();
    } catch (e) {
      setError(mapUserError(e));
      throw e;
    }
  }, [refreshSnapshot]);

  const clearError = useCallback(() => setError(""), []);

  return {
    tradingState,
    snapshot,
    fills,
    equityHistory,
    openOrders,
    connectorStatus,
    error,
    starting,
    stopping,
    startPaper,
    startLive,
    stopSession,
    control,
    refreshSnapshot,
    clearError,
    isActive: tradingState.mode !== "idle",
  };
}
