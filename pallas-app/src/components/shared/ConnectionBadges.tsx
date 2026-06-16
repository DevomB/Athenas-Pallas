import { Badge } from "@/components/ui/badge";
import type { ConnectorStatusDto } from "@/types";

interface Props {
  connected: boolean;
  connectorStatus: ConnectorStatusDto["status"];
  paused: boolean;
  tradingEnabled: boolean;
}

export function ConnectionBadges({
  connected,
  connectorStatus,
  paused,
  tradingEnabled,
}: Props) {
  const connectionLabel =
    connectorStatus === "reconnecting"
      ? "Reconnecting"
      : connected
        ? "Connected"
        : "Disconnected";

  const connectionVariant =
    connectorStatus === "reconnecting"
      ? "secondary"
      : connected
        ? "default"
        : "destructive";

  return (
    <>
      <Badge variant={connectionVariant}>{connectionLabel}</Badge>
      <Badge variant={paused ? "secondary" : "outline"}>
        {paused ? "Paused" : "Running"}
      </Badge>
      <Badge variant={tradingEnabled ? "default" : "destructive"}>
        {tradingEnabled ? "Trading enabled" : "Trading disabled"}
      </Badge>
    </>
  );
}
