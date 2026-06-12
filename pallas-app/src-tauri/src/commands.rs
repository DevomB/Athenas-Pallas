use crate::dto::{ConfigDto, FetchRequest, FillDto, RunResultDto};
use crate::session::AppSession;
use athenas_pallas::backtest::{
    report_to_dto, run_backtest_with_cancel, run_external_backtest_with_cancel, BacktestConfig,
};
use athenas_pallas::data::fetch::{binance, yahoo};
use rust_decimal::Decimal;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn load_config(path: String) -> Result<ConfigDto, String> {
    let cfg = BacktestConfig::load_toml(PathBuf::from(&path).as_path())
        .map_err(|e| e.to_string())?;
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
        .add_filter("Strategy", &["py", "exe"])
        .blocking_pick_file();
    Ok(file.map(|p| p.to_string()))
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
    session.cancel.store(false, std::sync::atomic::Ordering::SeqCst);
    let cancel = session.cancel.clone();
    let session_worker = session.inner().clone();
    let app_worker = app.clone();

    let handle = std::thread::spawn(move || {
        let result = (|| -> Result<RunResultDto, String> {
            let mut cfg = config.to_backtest_config()?;
            if cfg.data_path.as_os_str().is_empty() {
                return Err("data path is required".into());
            }
            if csv_row_count(&cfg.data_path)? > 50_000 {
                cfg.record_equity_curve = false;
            }
            let report = if let Some(ref strategy) = cfg.strategy_path {
                run_external_backtest_with_cancel(&cfg, strategy, Some(cancel.clone()))
                    .map_err(|e| e.to_string())?
            } else {
                run_backtest_with_cancel(&cfg, Some(cancel.clone())).map_err(|e| e.to_string())?
            };
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
                report: report_to_dto(&report, 2000),
                fills,
                full_report_json,
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
    inst.insert("exchange".into(), cfg.instrument.exchange.to_string().into());
    inst.insert("symbol".into(), cfg.instrument.symbol.to_string().into());
    inst.insert(
        "asset_class".into(),
        match cfg.asset_class {
            athenas_pallas::instrument::AssetClass::Equity => "equity",
            athenas_pallas::instrument::AssetClass::Forex => "forex",
            athenas_pallas::instrument::AssetClass::Future => "future",
            athenas_pallas::instrument::AssetClass::Crypto => "crypto",
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
    bt.insert("slippage_bps".into(), decimal_to_i64(cfg.slippage_bps).into());
    bt.insert(
        "half_spread_bps".into(),
        decimal_to_i64(cfg.half_spread_bps).into(),
    );
    bt.insert("periods_per_year".into(), cfg.periods_per_year.into());
    bt.insert(
        "record_equity_curve".into(),
        cfg.record_equity_curve.into(),
    );
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
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let rows = text.lines().filter(|l| !l.trim().is_empty()).count();
    Ok(rows.saturating_sub(1))
}
