import type { PaperSessionConfigDto } from "@/types";

export function validatePaperSession(config: PaperSessionConfigDto): string | null {
  if (!config.symbol.trim()) {
    return "Symbol is required";
  }
  if (!config.exchange.trim()) {
    return "Exchange is required";
  }
  const amount = Number(config.starting_balance_amount);
  if (!Number.isFinite(amount) || amount <= 0) {
    return "Starting balance must be a positive number";
  }
  if (config.strategy_path?.trim()) {
    const path = config.strategy_path.trim();
    if (path.includes("..")) {
      return "Strategy path must not contain parent directory references";
    }
  }
  return null;
}
