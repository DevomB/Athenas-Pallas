mod commands;
mod dto;
mod session;

use session::AppSession;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(Arc::new(AppSession::new()));
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
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}
