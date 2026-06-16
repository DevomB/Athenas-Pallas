import type { ConfigDto } from "../types";

export function validateConfig(config: ConfigDto): string | null {
  const fields = validateConfigFields(config);
  const first = Object.values(fields)[0];
  return first ?? null;
}

export function validateConfigFields(
  config: ConfigDto,
): Record<string, string> {
  const errors: Record<string, string> = {};
  if (!config.data_path.trim()) {
    errors.data_path = "Data path is required";
  }
  if (!config.symbol.trim()) {
    errors.symbol = "Symbol is required";
  }
  if (!config.exchange.trim()) {
    errors.exchange = "Exchange is required";
  }
  if (!Number.isFinite(config.fee_bps) || config.fee_bps < 0) {
    errors.fee_bps = "Fee bps must be a non-negative number";
  }
  if (!Number.isFinite(config.slippage_bps) || config.slippage_bps < 0) {
    errors.slippage_bps = "Slippage bps must be a non-negative number";
  }
  if (!Number.isFinite(config.half_spread_bps) || config.half_spread_bps < 0) {
    errors.half_spread_bps = "Half spread bps must be a non-negative number";
  }
  if (!Number.isFinite(config.periods_per_year) || config.periods_per_year <= 0) {
    errors.periods_per_year = "Periods per year must be a positive number";
  }
  return errors;
}
