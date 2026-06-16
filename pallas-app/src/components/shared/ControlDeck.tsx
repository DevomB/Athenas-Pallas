import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  Ban,
  CircleStop,
  Pause,
  Play,
  ShieldAlert,
  XCircle,
} from "lucide-react";

interface Props {
  disabled?: boolean;
  paused?: boolean;
  tradingEnabled?: boolean;
  onPause: () => void;
  onResume: () => void;
  onTradingEnable: () => void;
  onTradingDisable: () => void;
  onCancelAll: () => void;
  onFlatten: () => void;
}

export function ControlDeck({
  disabled,
  paused,
  tradingEnabled,
  onPause,
  onResume,
  onTradingEnable,
  onTradingDisable,
  onCancelAll,
  onFlatten,
}: Props) {
  return (
    <div className="flex flex-wrap gap-2 rounded-lg border bg-card p-3">
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="secondary"
            size="sm"
            disabled={disabled || paused}
            onClick={onPause}
          >
            <Pause data-icon="inline-start" />
            Pause
          </Button>
        </TooltipTrigger>
        <TooltipContent>Stop new orders; keep processing market data</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="secondary"
            size="sm"
            disabled={disabled || !paused}
            onClick={onResume}
          >
            <Play data-icon="inline-start" />
            Resume
          </Button>
        </TooltipTrigger>
        <TooltipContent>Resume order submission</TooltipContent>
      </Tooltip>
      {tradingEnabled ? (
        <Button
          variant="outline"
          size="sm"
          disabled={disabled}
          onClick={onTradingDisable}
        >
          <Ban data-icon="inline-start" />
          Disable trading
        </Button>
      ) : (
        <Button
          variant="outline"
          size="sm"
          disabled={disabled}
          onClick={onTradingEnable}
        >
          <Play data-icon="inline-start" />
          Enable trading
        </Button>
      )}
      <Button
        variant="outline"
        size="sm"
        disabled={disabled}
        onClick={onCancelAll}
      >
        <XCircle data-icon="inline-start" />
        Cancel all
      </Button>
      <Button
        variant="destructive"
        size="sm"
        disabled={disabled}
        onClick={onFlatten}
      >
        <ShieldAlert data-icon="inline-start" />
        Flatten
      </Button>
      {disabled && (
        <p className="flex w-full items-center gap-2 text-xs text-muted-foreground">
          <CircleStop className="size-3" />
          Start a session to use runtime controls
        </p>
      )}
    </div>
  );
}
