export type AppRoute =
  | "quick-start"
  | "backtest"
  | "paper"
  | "live"
  | "data-studio"
  | "results"
  | "settings";

export interface BalanceDto {
  asset: string;
  amount: string;
}

export interface ExtraInstrumentDto {
  exchange: string;
  symbol: string;
  asset_class: string;
  data_path?: string | null;
  data_format?: string | null;
  lot_size?: string | null;
  tick_size?: string | null;
  contract_multiplier?: string | null;
  expiry?: string | null;
  margin_initial_rate?: string | null;
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
  bar_interval?: string | null;
  session_filter?: string | null;
  auto_periods_per_year?: boolean;
  risk_free_annual?: number;
  max_position_abs?: string | null;
  max_daily_loss_quote?: string | null;
  margin_initial_rate?: string | null;
  lot_size?: string | null;
  tick_size?: string | null;
  contract_multiplier?: string | null;
  expiry?: string | null;
  record_equity_curve: boolean;
  strategy_path?: string | null;
  python_exe: string;
  output_path?: string | null;
  balances: BalanceDto[];
  extra_instruments?: ExtraInstrumentDto[];
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
  win_rate: number;
  profit_factor: number;
  closed_trades: number;
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

export interface StrategyResolutionDto {
  kind: string;
  path: string;
}

export interface FetchRequest {
  provider: string;
  symbol: string;
  interval: string;
  days: number;
  output_path: string;
}

export interface ResampleRequest {
  input_path: string;
  target_interval: string;
  output_path: string;
}

export interface MergeSourceDto {
  format: string;
  exchange: string;
  symbol: string;
  path: string;
}

export interface MergeRequest {
  sources: MergeSourceDto[];
  output_path: string;
}

export interface CsvPreviewDto {
  headers: string[];
  head_rows: string[][];
  tail_rows: string[][];
  total_rows: number;
}

export interface PaperSessionConfigDto {
  exchange: string;
  symbol: string;
  fee_bps: number;
  slippage_bps: number;
  starting_balance_asset: string;
  starting_balance_amount: string;
  strategy_path?: string | null;
  python_exe: string;
}

export interface LiveSessionConfigDto extends PaperSessionConfigDto {
  use_testnet: boolean;
}

export interface OpenOrderDto {
  id: string;
  instrument: string;
  side: string;
  order_type: string;
  price?: string | null;
  stop_price?: string | null;
  remaining_qty: string;
  original_qty: string;
  status: string;
}

export interface PositionDto {
  instrument: string;
  qty: string;
  mark_price?: string | null;
  notional?: string | null;
}

export interface BalanceSnapshotDto {
  asset: string;
  amount: string;
}

export interface PositionsSnapshotDto {
  balances: BalanceSnapshotDto[];
  positions: PositionDto[];
  equity: string;
  mark_price?: string | null;
  paused: boolean;
  trading_enabled: boolean;
  connected: boolean;
}

export interface ConnectorStatusDto {
  status: "connected" | "disconnected" | "reconnecting";
  instrument: string;
}

export interface LiveEquityPoint {
  time: number;
  equity: number;
}

export interface TradingStateDto {
  mode: "idle" | "paper" | "live";
  instrument: string;
  paused: boolean;
  trading_enabled: boolean;
  connected: boolean;
}

export interface FillEventDto {
  ts: string;
  instrument: string;
  side: string;
  qty: string;
  price: string;
  fee: string;
}

export interface CredentialsDto {
  api_key: string;
  api_secret: string;
}

export interface SweepRequest {
  base_config_path: string;
  sweep_path: string;
  output_path: string;
}

export interface SweepResultRow {
  name: string;
  pnl: number;
  sharpe: number;
  sortino: number;
  max_drawdown: number;
  closed_trades: number;
  win_rate: number;
  profit_factor: number;
}

export interface SweepResultDto {
  rows: SweepResultRow[];
  output_path: string;
}

export interface ApplySweepRequest {
  base_config_path: string;
  sweep_path: string;
  row_name: string;
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
  auto_periods_per_year: true,
  risk_free_annual: 0,
  record_equity_curve: true,
  strategy_path: "simple_sma",
  python_exe: "python",
  balances: [{ asset: "USDT", amount: "10000" }],
  extra_instruments: [],
});

export const defaultPaperConfig = (): PaperSessionConfigDto => ({
  exchange: "binance",
  symbol: "BTCUSDT",
  fee_bps: 10,
  slippage_bps: 5,
  starting_balance_asset: "USDT",
  starting_balance_amount: "10000",
  strategy_path: null,
  python_exe: "python",
});

export const defaultLiveConfig = (): LiveSessionConfigDto => ({
  ...defaultPaperConfig(),
  use_testnet: true,
});
