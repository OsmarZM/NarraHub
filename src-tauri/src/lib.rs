pub mod commands;
pub mod database;
mod sync;

use database::migrations::{MIGRATION_V1, MIGRATION_V2, MIGRATION_V3, MIGRATION_V4, MIGRATION_V5};
use tauri_plugin_sql::{Migration, MigrationKind};

fn has_updater_config(config: &tauri::Config) -> bool {
    config
        .plugins
        .0
        .get("updater")
        .is_some_and(serde_json::Value::is_object)
}

#[tauri::command]
fn updater_configured(app: tauri::AppHandle) -> bool {
    has_updater_config(app.config())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(std::sync::Mutex::new(sync::SyncState::default()))
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(desktop)]
            if has_updater_config(app.config()) {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            }
            Ok(())
        })
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations(
                    "sqlite:narrahub.db",
                    vec![
                        Migration {
                            version: 1,
                            description: "Create NarraHub schema",
                            sql: MIGRATION_V1,
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 2,
                            description: "Add planning, history and sync foundation",
                            sql: MIGRATION_V2,
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 3,
                            description: "Link timeline events and support fictional dates",
                            sql: MIGRATION_V3,
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 4,
                            description: "Add reusable image galleries",
                            sql: MIGRATION_V4,
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 5,
                            description: "Add book cover images",
                            sql: MIGRATION_V5,
                            kind: MigrationKind::Up,
                        },
                    ],
                )
                .build(),
        )
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::universe_commands::get_app_info,
            updater_configured,
            sync::sync_status,
            sync::sync_start,
            sync::sync_stop,
            sync::sync_connect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NarraHub");
}
