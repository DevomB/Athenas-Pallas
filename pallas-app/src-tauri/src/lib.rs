mod commands;
mod credentials;
mod data_tools;
mod dto;
mod session;
mod system_config;
mod trading_session;

use session::AppSession;
use std::sync::Arc;
use tauri::Manager;
use trading_session::TradingSessionManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(Arc::new(AppSession::new()));
            app.manage(Arc::new(TradingSessionManager::new()));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if let Some(session) = window.app_handle().try_state::<Arc<AppSession>>() {
                    session.join_with_timeout(std::time::Duration::from_secs(2));
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_config,
            commands::pick_csv,
            commands::pick_toml,
            commands::pick_strategy,
            commands::fetch_bars,
            commands::run_backtest,
            commands::stop_run,
            commands::export_report,
            commands::save_config_toml,
            commands::pick_save_toml,
            commands::session_shutdown,
            commands::resample_bars,
            commands::merge_bars,
            commands::preview_csv,
            commands::start_paper_session,
            commands::start_live_session,
            commands::stop_trading_session,
            commands::trading_pause,
            commands::trading_resume,
            commands::trading_enable,
            commands::trading_disable,
            commands::cancel_all_orders,
            commands::flatten_all,
            commands::get_positions_snapshot,
            commands::list_open_orders,
            commands::save_credentials,
            commands::get_credentials,
            commands::run_parameter_sweep,
            commands::apply_sweep_row,
            commands::load_system_config,
            commands::save_system_config,
            commands::load_system_config_example,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}
