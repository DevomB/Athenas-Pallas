import type { ConfigDto } from "../types";

export function validateConfig(config: ConfigDto): string | null {
  if (!config.data_path.trim()) {
    return "Data path is required";
  }
  if (!config.symbol.trim()) {
    return "Symbol is required";
  }
  if (!config.exchange.trim()) {
    return "Exchange is required";
  }
  if (!Number.isFinite(config.fee_bps) || config.fee_bps < 0) {
    return "Fee bps must be a non-negative number";
  }
  if (!Number.isFinite(config.slippage_bps) || config.slippage_bps < 0) {
    return "Slippage bps must be a non-negative number";
  }
  if (!Number.isFinite(config.half_spread_bps) || config.half_spread_bps < 0) {
    return "Half spread bps must be a non-negative number";
  }
  if (!Number.isFinite(config.periods_per_year) || config.periods_per_year <= 0) {
    return "Periods per year must be a positive number";
  }
  return null;
}
