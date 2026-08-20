pub mod commands;
pub mod database;
mod sync;

use database::migrations::{MIGRATION_V1, MIGRATION_V2};
use tauri_plugin_sql::{Migration, MigrationKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(std::sync::Mutex::new(sync::SyncState::default()))
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;
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
                    ],
                )
                .build(),
        )
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::universe_commands::get_app_info,
            sync::sync_status,
            sync::sync_start,
            sync::sync_stop,
            sync::sync_connect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NarraHub");
}
