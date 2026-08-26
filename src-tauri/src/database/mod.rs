pub mod backup;
pub mod error;
pub mod health;
pub mod migrations;
pub mod planning;
pub mod production_replica;
pub mod recovery;

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub fn app_data_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory)
}

pub fn app_database_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_path(app)?.join("narrahub.db"))
}
