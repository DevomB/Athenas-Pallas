use crate::dto::CredentialsDto;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct StoredCredentials {
    api_key: String,
    api_secret: String,
}

fn creds_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("credentials.json"))
}

pub fn save_credentials(app: &AppHandle, credentials: CredentialsDto) -> Result<(), String> {
    let path = creds_path(app)?;
    let stored = StoredCredentials {
        api_key: credentials.api_key,
        api_secret: credentials.api_secret,
    };
    let json = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_credentials(app: &AppHandle) -> Result<Option<CredentialsDto>, String> {
    let path = creds_path(app)?;
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let stored: StoredCredentials = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if stored.api_key.is_empty() {
        return Ok(None);
    }
    Ok(Some(CredentialsDto {
        api_key: stored.api_key,
        api_secret: stored.api_secret,
    }))
}

pub fn load_credentials_for_live(app: &AppHandle) -> Result<(String, String), String> {
    match get_credentials(app)? {
        Some(c) if !c.api_key.is_empty() => Ok((c.api_key, c.api_secret)),
        _ => Err("Binance API credentials not configured".into()),
    }
}
