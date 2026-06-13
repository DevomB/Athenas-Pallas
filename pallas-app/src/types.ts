export interface BalanceDto {
  asset: string;
  amount: string;
}

export interface ConfigDto {
  data_path: string;
  data_format: string;
  exchange: string;
  symbol: string;
  asset_class: string;
  fee_bps: number;
  slippage_bps: number;
  half_spread_bps: number;
  periods_per_year: number;
  lot_size?: string | null;
  tick_size?: string | null;
  contract_multiplier?: string | null;
  expiry?: string | null;
  record_equity_curve: boolean;
  strategy_path?: string | null;
  python_exe: string;
  output_path?: string | null;
  balances: BalanceDto[];
}

export interface EquityPointDto {
  ts_unix_ms: number;
  equity_f64: number;
}

export interface BacktestReportDto {
  pnl: number;
  pnl_pct: number;
  max_drawdown: number;
  sharpe: number;
  sortino: number;
  fill_count: number;
  wall_time_ms: number;
  equity_curve: EquityPointDto[];
}

export interface FillDto {
  ts: string;
  side: string;
  qty: string;
  price: string;
  fee: string;
}

export interface RunResultDto {
  report: BacktestReportDto;
  fills: FillDto[];
  full_report_json: string;
  equity_curve_skipped: boolean;
  equity_curve_downsampled: boolean;
}

export interface FetchRequest {
  provider: string;
  symbol: string;
  interval: string;
  days: number;
  output_path: string;
}

export const defaultConfig = (): ConfigDto => ({
  data_path: "data/BTCUSDT_live.csv",
  data_format: "ohlcv",
  exchange: "binance",
  symbol: "BTCUSDT",
  asset_class: "crypto",
  fee_bps: 10,
  slippage_bps: 5,
  half_spread_bps: 5,
  periods_per_year: 365,
  record_equity_curve: true,
  strategy_path: "trading/strategies/simple_sma/strategy.py",
  python_exe: "python",
  balances: [{ asset: "USDT", amount: "10000" }],
});
