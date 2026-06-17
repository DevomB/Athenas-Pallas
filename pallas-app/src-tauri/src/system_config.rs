use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("system_config.json"))
}

pub fn load_system_config(app: &AppHandle) -> Result<String, String> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(example_system_config());
    }
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

pub fn save_system_config(app: &AppHandle, json: String) -> Result<(), String> {
    serde_json::from_str::<serde_json::Value>(&json).map_err(|e| format!("Invalid JSON: {e}"))?;
    let path = config_path(app)?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

pub fn example_system_config() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../athenas-pallas/examples/system_config.json");
    std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into())
}
