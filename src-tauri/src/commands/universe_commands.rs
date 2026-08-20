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
        version: "0.2.0".to_string(),
        local_storage_path: "narrahub.db".to_string(),
    }
}
