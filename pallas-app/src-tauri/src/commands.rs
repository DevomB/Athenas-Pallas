use crate::credentials::{
    get_credentials as read_stored_credentials, load_credentials_for_live,
    save_credentials as write_stored_credentials,
};
use crate::data_tools::{
    merge_bars as merge_bars_file, preview_csv as preview_csv_file,
    resample_bars as resample_bars_file,
};
use crate::dto::{
    ApplySweepRequest, ConfigDto, CredentialsDto, FetchRequest, FillDto, LiveSessionConfigDto,
    MergeRequest, OpenOrderDto, PaperSessionConfigDto, ResampleRequest, RunResultDto,
    StrategyResolutionDto, SweepRequest, SweepResultDto, SweepResultRow,
};
use crate::session::AppSession;
use crate::trading_session::TradingSessionManager;
use athenas_pallas::backtest::{
    report_to_dto, resolve_strategy_path, run_backtest as engine_run_backtest,
    run_backtest_with_cancel, run_external_backtest_with_cancel, BacktestConfig,
};
use athenas_pallas::data::fetch::{binance, yahoo};
use athenas_pallas::events::ControlEvent;
use rust_decimal::Decimal;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn load_config(path: String) -> Result<ConfigDto, String> {
    let cfg =
        BacktestConfig::load_toml(PathBuf::from(&path).as_path()).map_err(|e| e.to_string())?;
    Ok(ConfigDto::from_backtest_config(&cfg))
}

#[tauri::command]
pub async fn pick_csv(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let file = app
        .dialog()
        .file()
        .add_filter("CSV", &["csv"])
        .blocking_pick_file();
    Ok(file.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn pick_toml(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let file = app
        .dialog()
        .file()
        .add_filter("TOML", &["toml"])
        .blocking_pick_file();
    Ok(file.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn pick_strategy(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let file = app
        .dialog()
        .file()
        .add_filter("Strategy file", &["py", "exe"])
        .blocking_pick_file();
    Ok(file.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn pick_strategy_dir(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let folder = app.dialog().file().blocking_pick_folder();
    Ok(folder.map(|p| p.to_string()))
}

#[tauri::command]
pub fn detect_strategy(path: String) -> Result<StrategyResolutionDto, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("strategy path is empty".into());
    }
    let resolved =
        resolve_strategy_path(PathBuf::from(trimmed).as_path()).map_err(|e| e.to_string())?;
    Ok(StrategyResolutionDto {
        kind: resolved.kind().to_string(),
        path: resolved.path().display().to_string(),
    })
}

#[tauri::command]
pub async fn fetch_bars(app: AppHandle, req: FetchRequest) -> Result<String, String> {
    let output = PathBuf::from(&req.output_path);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("fetch-progress", "starting");
    let client = reqwest::Client::new();
    match req.provider.as_str() {
        "yahoo" => {
            let range = format!("{}d", req.days);
            let _ = app.emit("fetch-progress", format!("yahoo {} {}", req.symbol, range));
            yahoo::fetch_chart_csv(&client, &req.symbol, &req.interval, &range, &output)
                .await
                .map_err(|e| e.to_string())?;
        }
        "binance" => {
            let end = time::OffsetDateTime::now_utc();
            let start = end - time::Duration::days(req.days as i64);
            let _ = app.emit(
                "fetch-progress",
                format!("binance {} {}d", req.symbol, req.days),
            );
            binance::fetch_klines_csv(
                &client,
                &req.symbol.to_uppercase(),
                &req.interval,
                start.unix_timestamp() * 1000,
                end.unix_timestamp() * 1000,
                &output,
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        other => return Err(format!("unknown provider: {other}")),
    }
    let _ = app.emit("fetch-progress", "done");
    Ok(output.display().to_string())
}

#[tauri::command]
pub async fn run_backtest(
    app: AppHandle,
    session: State<'_, Arc<AppSession>>,
    config: ConfigDto,
) -> Result<(), String> {
    if !session.try_start() {
        return Err("backtest already running".into());
    }
    session
        .cancel
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let cancel = session.cancel.clone();
    let session_worker = session.inner().clone();
    let app_worker = app.clone();

    let handle = std::thread::spawn(move || {
        let result = (|| -> Result<RunResultDto, String> {
            let _ = app_worker.emit("run-progress", "preparing config...");
            let mut cfg = config.to_backtest_config()?;
            let progress_app = app_worker.clone();
            cfg.on_progress = Some(Arc::new(move |msg: &str| {
                let _ = progress_app.emit("run-progress", msg);
            }));
            if cfg.data_path.as_os_str().is_empty() {
                return Err("data path is required".into());
            }
            let _ = app_worker.emit("run-progress", "loading data...");
            let row_count = csv_row_count(&cfg.data_path).unwrap_or(0);
            let equity_curve_downsampled = row_count > 50_000;
            let max_chart_points = 2000usize;
            cfg.record_equity_curve = true;
            let _ = app_worker.emit("run-progress", "running backtest...");
            let report = if let Some(ref strategy) = cfg.strategy_path {
                run_external_backtest_with_cancel(&cfg, strategy, Some(cancel.clone()))
                    .map_err(|e| e.to_string())?
            } else {
                run_backtest_with_cancel(&cfg, Some(cancel.clone())).map_err(|e| e.to_string())?
            };
            let equity_curve_downsampled =
                report.equity_curve.len() > max_chart_points || equity_curve_downsampled;
            let equity_curve_skipped = report.equity_curve.is_empty();
            let _ = app_worker.emit("run-progress", "building report...");
            let full_report_json =
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
            let fills: Vec<FillDto> = report
                .fills
                .iter()
                .map(|f| FillDto {
                    ts: f.ts.to_string(),
                    side: format!("{:?}", f.side),
                    qty: f.qty.clone(),
                    price: f.price.clone(),
                    fee: f.fee.clone(),
                })
                .collect();
            Ok(RunResultDto {
                report: report_to_dto(&report, max_chart_points),
                fills,
                full_report_json,
                equity_curve_skipped,
                equity_curve_downsampled,
            })
        })();

        match result {
            Ok(dto) => {
                let _ = app_worker.emit("run-finished", dto);
            }
            Err(e) => {
                let _ = app_worker.emit("run-failed", e);
            }
        }
        session_worker.finish_run();
    });
    session.set_join(handle);

    Ok(())
}

#[tauri::command]
pub fn stop_run(session: State<'_, Arc<AppSession>>) -> Result<(), String> {
    session.stop_run();
    Ok(())
}

#[tauri::command]
pub async fn export_report(app: AppHandle, json: String) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;
    let file = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .set_file_name("backtest_report.json")
        .blocking_save_file();
    let Some(path) = file else {
        return Ok(());
    };
    std::fs::write(path.to_string(), json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn pick_save_toml(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let file = app
        .dialog()
        .file()
        .add_filter("TOML", &["toml"])
        .set_file_name("backtest.toml")
        .blocking_save_file();
    Ok(file.map(|p| p.to_string()))
}

#[tauri::command]
pub fn save_config_toml(path: String, config: ConfigDto) -> Result<(), String> {
    let cfg = config.to_backtest_config()?;
    let mut table = toml::map::Map::new();

    let mut inst = toml::map::Map::new();
    inst.insert(
        "exchange".into(),
        cfg.instrument.exchange.to_string().into(),
    );
    inst.insert("symbol".into(), cfg.instrument.symbol.to_string().into());
    inst.insert(
        "asset_class".into(),
        match cfg.asset_class {
            athenas_pallas::instrument::AssetClass::Equity => "equity",
            athenas_pallas::instrument::AssetClass::Forex => "forex",
            athenas_pallas::instrument::AssetClass::Future => "future",
            athenas_pallas::instrument::AssetClass::Crypto => "crypto",
            athenas_pallas::instrument::AssetClass::Option => "option",
            athenas_pallas::instrument::AssetClass::Perpetual => "perpetual",
            athenas_pallas::instrument::AssetClass::Bond => "bond",
            athenas_pallas::instrument::AssetClass::Hybrid => "hybrid",
        }
        .into(),
    );
    if let Some(v) = &cfg.lot_size {
        inst.insert("lot_size".into(), v.to_string().into());
    }
    if let Some(v) = &cfg.tick_size {
        inst.insert("tick_size".into(), v.to_string().into());
    }
    if let Some(v) = &cfg.contract_multiplier {
        inst.insert("contract_multiplier".into(), v.to_string().into());
    }
    if let Some(v) = &cfg.expiry {
        inst.insert("expiry".into(), v.clone().into());
    }
    table.insert("instrument".into(), inst.into());

    let mut bt = toml::map::Map::new();
    bt.insert("data".into(), cfg.data_path.display().to_string().into());
    bt.insert(
        "data_format".into(),
        match cfg.data_format {
            athenas_pallas::backtest::DataFormat::Ohlcv => "ohlcv",
            athenas_pallas::backtest::DataFormat::Yahoo => "yahoo",
            athenas_pallas::backtest::DataFormat::Fx => "fx",
            athenas_pallas::backtest::DataFormat::Future => "future",
            athenas_pallas::backtest::DataFormat::Auto => "auto",
        }
        .into(),
    );
    bt.insert("fee_bps".into(), decimal_to_i64(cfg.fee_bps).into());
    bt.insert(
        "slippage_bps".into(),
        decimal_to_i64(cfg.slippage_bps).into(),
    );
    bt.insert(
        "half_spread_bps".into(),
        decimal_to_i64(cfg.half_spread_bps).into(),
    );
    bt.insert("periods_per_year".into(), cfg.periods_per_year.into());
    bt.insert("record_equity_curve".into(), cfg.record_equity_curve.into());
    if let Some(v) = &cfg.bar_interval {
        bt.insert("bar_interval".into(), v.clone().into());
    }
    if let Some(v) = &cfg.session_filter {
        bt.insert("session_filter".into(), v.clone().into());
    }
    bt.insert(
        "auto_periods_per_year".into(),
        cfg.auto_periods_per_year.into(),
    );
    if cfg.risk_free_annual != 0.0 {
        bt.insert("risk_free_annual".into(), cfg.risk_free_annual.into());
    }
    if let Some(v) = &cfg.max_position_abs {
        bt.insert("max_position_abs".into(), v.to_string().into());
    }
    if let Some(v) = &cfg.max_daily_loss_quote {
        bt.insert("max_daily_loss_quote".into(), v.to_string().into());
    }
    if let Some(v) = &cfg.margin_initial_rate {
        bt.insert("margin_initial_rate".into(), v.to_string().into());
    }
    if let Some(p) = &cfg.strategy_path {
        bt.insert("strategy".into(), p.display().to_string().into());
    }
    bt.insert("python".into(), cfg.python_exe.into());
    if let Some(p) = &cfg.output_path {
        bt.insert("output".into(), p.display().to_string().into());
    }
    table.insert("backtest".into(), bt.into());

    let balances: Vec<toml::Value> = cfg
        .balances
        .iter()
        .map(|(a, v)| {
            let mut row = toml::map::Map::new();
            row.insert("asset".into(), a.0.to_string().into());
            row.insert("amount".into(), v.to_string().into());
            row.into()
        })
        .collect();
    table.insert("balances".into(), balances.into());

    if !cfg.extra_instruments.is_empty() {
        let extras: Vec<toml::Value> = cfg
            .extra_instruments
            .iter()
            .map(|e| {
                let mut row = toml::map::Map::new();
                row.insert("exchange".into(), e.instrument.exchange.clone().into());
                row.insert("symbol".into(), e.instrument.symbol.clone().into());
                row.insert(
                    "asset_class".into(),
                    match e.asset_class {
                        athenas_pallas::instrument::AssetClass::Equity => "equity",
                        athenas_pallas::instrument::AssetClass::Forex => "forex",
                        athenas_pallas::instrument::AssetClass::Future => "future",
                        athenas_pallas::instrument::AssetClass::Crypto => "crypto",
                        athenas_pallas::instrument::AssetClass::Option => "option",
                        athenas_pallas::instrument::AssetClass::Perpetual => "perpetual",
                        athenas_pallas::instrument::AssetClass::Bond => "bond",
                        athenas_pallas::instrument::AssetClass::Hybrid => "hybrid",
                    }
                    .into(),
                );
                if let Some(p) = &e.data_path {
                    row.insert("data".into(), p.display().to_string().into());
                }
                if let Some(f) = e.data_format {
                    row.insert(
                        "data_format".into(),
                        match f {
                            athenas_pallas::backtest::DataFormat::Ohlcv => "ohlcv",
                            athenas_pallas::backtest::DataFormat::Yahoo => "yahoo",
                            athenas_pallas::backtest::DataFormat::Fx => "fx",
                            athenas_pallas::backtest::DataFormat::Future => "future",
                            athenas_pallas::backtest::DataFormat::Auto => "auto",
                        }
                        .into(),
                    );
                }
                row.into()
            })
            .collect();
        table.insert("instruments".into(), extras.into());
    }

    let text = toml::to_string_pretty(&table).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn session_shutdown(session: State<'_, Arc<AppSession>>) {
    session.shutdown();
}

fn decimal_to_i64(d: Decimal) -> i64 {
    d.mantissa().unsigned_abs() as i64 / 10i64.pow(d.scale())
}

fn csv_row_count(path: &std::path::Path) -> Result<usize, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let rows = BufReader::new(file)
        .lines()
        .try_fold(0usize, |count, line| {
            let line = line?;
            Ok::<usize, std::io::Error>(count + usize::from(!line.trim().is_empty()))
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.saturating_sub(1))
}

#[tauri::command]
pub fn resample_bars(req: ResampleRequest) -> Result<String, String> {
    resample_bars_file(&req)
}

#[tauri::command]
pub fn merge_bars(req: MergeRequest) -> Result<String, String> {
    merge_bars_file(&req)
}

#[tauri::command]
pub fn preview_csv(path: String) -> Result<crate::dto::CsvPreviewDto, String> {
    preview_csv_file(&path)
}

#[tauri::command]
pub fn start_paper_session(
    app: AppHandle,
    trading: State<'_, Arc<TradingSessionManager>>,
    config: PaperSessionConfigDto,
) -> Result<(), String> {
    trading.start_paper(app, config)
}

#[tauri::command]
pub fn start_live_session(
    app: AppHandle,
    trading: State<'_, Arc<TradingSessionManager>>,
    config: LiveSessionConfigDto,
) -> Result<(), String> {
    let (api_key, api_secret) = load_credentials_for_live(&app)?;
    trading.start_live(app, config, api_key, api_secret)
}

#[tauri::command]
pub fn stop_trading_session(
    app: AppHandle,
    trading: State<'_, Arc<TradingSessionManager>>,
) -> Result<(), String> {
    trading.stop_with_emit(&app);
    Ok(())
}

#[tauri::command]
pub fn trading_pause(
    app: AppHandle,
    trading: State<'_, Arc<TradingSessionManager>>,
) -> Result<(), String> {
    trading.send_control(&app, ControlEvent::Pause)
}

#[tauri::command]
pub fn trading_resume(
    app: AppHandle,
    trading: State<'_, Arc<TradingSessionManager>>,
) -> Result<(), String> {
    trading.send_control(&app, ControlEvent::Resume)
}

#[tauri::command]
pub fn trading_enable(
    app: AppHandle,
    trading: State<'_, Arc<TradingSessionManager>>,
) -> Result<(), String> {
    trading.send_control(&app, ControlEvent::EnableTrading)
}

#[tauri::command]
pub fn trading_disable(
    app: AppHandle,
    trading: State<'_, Arc<TradingSessionManager>>,
) -> Result<(), String> {
    trading.send_control(&app, ControlEvent::DisableTrading)
}

#[tauri::command]
pub fn cancel_all_orders(trading: State<'_, Arc<TradingSessionManager>>) -> Result<(), String> {
    trading.cancel_all()
}

#[tauri::command]
pub fn flatten_all(trading: State<'_, Arc<TradingSessionManager>>) -> Result<(), String> {
    trading.flatten_all()
}

#[tauri::command]
pub fn get_positions_snapshot(
    trading: State<'_, Arc<TradingSessionManager>>,
) -> Result<crate::dto::PositionsSnapshotDto, String> {
    trading.snapshot()
}

#[tauri::command]
pub fn list_open_orders(
    trading: State<'_, Arc<TradingSessionManager>>,
) -> Result<Vec<OpenOrderDto>, String> {
    let orders = trading.list_open_orders()?;
    Ok(orders
        .into_iter()
        .map(|o| OpenOrderDto {
            id: format!("{:?}", o.id),
            instrument: format!("{}:{}", o.instrument.exchange, o.instrument.symbol),
            side: format!("{:?}", o.side),
            order_type: format!("{:?}", o.order_type),
            price: o.price.map(|d| d.to_string()),
            stop_price: o.stop_price.map(|d| d.to_string()),
            remaining_qty: o.remaining_qty.to_string(),
            original_qty: o.original_qty.to_string(),
            status: format!("{:?}", o.status),
        })
        .collect())
}

#[tauri::command]
pub fn save_credentials(app: AppHandle, credentials: CredentialsDto) -> Result<(), String> {
    write_stored_credentials(&app, credentials)
}

#[tauri::command]
pub fn get_credentials(app: AppHandle) -> Result<Option<CredentialsDto>, String> {
    read_stored_credentials(&app)
}

#[tauri::command]
pub async fn run_parameter_sweep(
    app: AppHandle,
    req: SweepRequest,
) -> Result<SweepResultDto, String> {
    let app_worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || run_parameter_sweep_sync(&app_worker, req))
        .await
        .map_err(|e| e.to_string())?
}

fn run_parameter_sweep_sync(app: &AppHandle, req: SweepRequest) -> Result<SweepResultDto, String> {
    #[derive(serde::Deserialize)]
    struct SweepFile {
        sweep: Vec<SweepRow>,
    }
    #[derive(serde::Deserialize)]
    struct SweepRow {
        name: String,
        #[serde(flatten)]
        overrides: toml::Table,
    }

    let base_txt = std::fs::read_to_string(&req.base_config_path).map_err(|e| e.to_string())?;
    let base: toml::Table = toml::from_str(&base_txt).map_err(|e| e.to_string())?;
    let sweep_file: SweepFile =
        toml::from_str(&std::fs::read_to_string(&req.sweep_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;

    let mut rows = Vec::new();
    let total = sweep_file.sweep.len();
    for (index, row) in sweep_file.sweep.iter().enumerate() {
        let _ = app.emit(
            "sweep-progress",
            format!("Running {} ({}/{})", row.name, index + 1, total),
        );
        let mut table = base.clone();
        for (k, v) in &row.overrides {
            table.insert(k.clone(), v.clone());
        }
        let merged = toml::to_string(&table).map_err(|e| e.to_string())?;
        let tmp = std::env::temp_dir().join(format!("pallas-sweep-{}.toml", row.name));
        std::fs::write(&tmp, merged).map_err(|e| e.to_string())?;
        let cfg = BacktestConfig::load_toml(tmp.as_path()).map_err(|e| e.to_string())?;
        let report = engine_run_backtest(&cfg).map_err(|e| e.to_string())?;
        let pnl: f64 = report.pnl.parse().unwrap_or(0.0);
        let _pnl_pct: f64 = report.pnl_pct.parse().unwrap_or(0.0);
        let max_dd = report.max_drawdown;
        rows.push(SweepResultRow {
            name: row.name.clone(),
            pnl,
            sharpe: report.sharpe,
            sortino: report.sortino,
            max_drawdown: max_dd,
            closed_trades: report.closed_trades,
            win_rate: report.win_rate,
            profit_factor: report.profit_factor,
        });
    }

    if let Some(parent) = std::path::Path::new(&req.output_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut wtr = csv::Writer::from_path(&req.output_path).map_err(|e| e.to_string())?;
    wtr.write_record([
        "name",
        "pnl",
        "sharpe",
        "sortino",
        "max_drawdown",
        "closed_trades",
        "win_rate",
        "profit_factor",
    ])
    .map_err(|e| e.to_string())?;
    for r in &rows {
        wtr.write_record([
            r.name.clone(),
            r.pnl.to_string(),
            format!("{:.4}", r.sharpe),
            format!("{:.4}", r.sortino),
            format!("{:.4}", r.max_drawdown),
            r.closed_trades.to_string(),
            format!("{:.4}", r.win_rate),
            format!("{:.4}", r.profit_factor),
        ])
        .map_err(|e| e.to_string())?;
    }
    wtr.flush().map_err(|e| e.to_string())?;

    let _ = app.emit("sweep-progress", "complete");

    Ok(SweepResultDto {
        rows,
        output_path: req.output_path,
    })
}

#[tauri::command]
pub fn apply_sweep_row(req: ApplySweepRequest) -> Result<ConfigDto, String> {
    #[derive(serde::Deserialize)]
    struct SweepFile {
        sweep: Vec<SweepRow>,
    }
    #[derive(serde::Deserialize)]
    struct SweepRow {
        name: String,
        #[serde(flatten)]
        overrides: toml::Table,
    }

    let base_txt = std::fs::read_to_string(&req.base_config_path).map_err(|e| e.to_string())?;
    let base: toml::Table = toml::from_str(&base_txt).map_err(|e| e.to_string())?;
    let sweep_file: SweepFile =
        toml::from_str(&std::fs::read_to_string(&req.sweep_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let row = sweep_file
        .sweep
        .into_iter()
        .find(|r| r.name == req.row_name)
        .ok_or_else(|| format!("sweep row not found: {}", req.row_name))?;
    let mut table = base;
    for (k, v) in row.overrides {
        table.insert(k, v);
    }
    let merged = toml::to_string(&table).map_err(|e| e.to_string())?;
    let tmp = std::env::temp_dir().join(format!("pallas-sweep-apply-{}.toml", row.name));
    std::fs::write(&tmp, merged).map_err(|e| e.to_string())?;
    let cfg = BacktestConfig::load_toml(tmp.as_path()).map_err(|e| e.to_string())?;
    Ok(ConfigDto::from_backtest_config(&cfg))
}

#[tauri::command]
pub fn load_system_config(app: AppHandle) -> Result<String, String> {
    crate::system_config::load_system_config(&app)
}

#[tauri::command]
pub fn save_system_config(app: AppHandle, json: String) -> Result<(), String> {
    crate::system_config::save_system_config(&app, json)
}

#[tauri::command]
pub fn load_system_config_example() -> Result<String, String> {
    Ok(crate::system_config::example_system_config())
}
