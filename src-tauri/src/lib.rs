pub mod commands;
pub mod database;
mod local_ai;
mod online_share;
mod sync;

use database::migrations::{
    MIGRATION_V1, MIGRATION_V2, MIGRATION_V3, MIGRATION_V4, MIGRATION_V5, MIGRATION_V6,
    MIGRATION_V7, MIGRATION_V8, MIGRATION_V9,
};
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
    let app = tauri::Builder::default()
        .manage(std::sync::Mutex::new(sync::SyncState::default()))
        .manage(std::sync::Mutex::new(
            local_ai::LocalAiRuntimeState::default(),
        ))
        .manage(std::sync::Mutex::new(
            online_share::OnlineShareState::default(),
        ))
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_http::init())
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
                        Migration {
                            version: 6,
                            description: "Add chapter summaries and universal metadata",
                            sql: MIGRATION_V6,
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 7,
                            description: "Separate entity fields from universal tags",
                            sql: MIGRATION_V7,
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 8,
                            description: "Add dedicated entity summaries",
                            sql: MIGRATION_V8,
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 9,
                            description: "Add collaborative sharing review queue",
                            sql: MIGRATION_V9,
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
            local_ai::local_ai_status,
            local_ai::install_local_ai,
            local_ai::start_local_ai_engine,
            local_ai::restart_local_ai_engine,
            sync::sync_status,
            sync::sync_start,
            sync::sync_stop,
            sync::sync_connect,
            online_share::online_share_status,
            online_share::online_share_start,
            online_share::online_share_stop,
            online_share::online_share_create,
            online_share::online_share_contributions,
            online_share::online_share_revoke,
        ])
        .build(tauri::generate_context!())
        .expect("error while building NarraHub");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            local_ai::stop_on_exit(app_handle);
            online_share::stop_on_exit(app_handle);
        }
    });
}
