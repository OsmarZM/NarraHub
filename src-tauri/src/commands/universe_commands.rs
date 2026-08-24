// NarraHub — Rust Commands
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub local_storage_path: String,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: "NarraHub".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        local_storage_path: "narrahub.db".to_string(),
    }
}
