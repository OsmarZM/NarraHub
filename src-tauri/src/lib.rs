pub mod application;
pub mod database;
pub mod domain;
pub mod infrastructure;
pub mod interface;
mod local_ai;
mod online_share;
mod sync;

use database::migrations::{
    MIGRATION_V1, MIGRATION_V10, MIGRATION_V11, MIGRATION_V12, MIGRATION_V13, MIGRATION_V14,
    MIGRATION_V15, MIGRATION_V2, MIGRATION_V3, MIGRATION_V4, MIGRATION_V5, MIGRATION_V6,
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
        .manage(database::backup::BackupRuntimeState::default())
        .manage(database::recovery::RestoreRuntimeState::default())
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
            #[cfg(desktop)]
            {
                use tauri::Manager;
                if let Some(icon) = app.default_window_icon() {
                    for win in app.webview_windows().values() {
                        let _ = win.set_icon(icon.clone());
                    }
                }
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
                        Migration {
                            version: 10,
                            description: "Allow tags on timeline and planning previews",
                            sql: MIGRATION_V10,
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 11,
                            description: "Add typed planning card fields and images",
                            sql: MIGRATION_V11,
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 12,
                            description: "Normalize planning card relations",
                            sql: MIGRATION_V12,
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 13,
                            description: "Migrate legacy planning card relations",
                            sql: MIGRATION_V13,
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 14,
                            description: "Add free-form connections canvas",
                            sql: MIGRATION_V14,
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 15,
                            description: "Add scope to planning field definitions",
                            sql: MIGRATION_V15,
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
            // Core Rust (Fase 4) — leituras e estatísticas
            interface::tauri::universe_commands::universe_list,
            interface::tauri::universe_commands::universe_get,
            interface::tauri::universe_commands::universe_stats,
            interface::tauri::universe_commands::universe_create,
            interface::tauri::universe_commands::universe_update,
            interface::tauri::universe_commands::universe_delete,
            interface::tauri::workspace_commands::timeline_list,
            interface::tauri::workspace_commands::timeline_create,
            interface::tauri::workspace_commands::timeline_rename,
            interface::tauri::workspace_commands::timeline_delete,
            interface::tauri::workspace_commands::relations_list,
            interface::tauri::workspace_commands::relation_create,
            interface::tauri::workspace_commands::relation_delete,
            interface::tauri::workspace_commands::history_list,
            interface::tauri::canvas_commands::canvas_nodes,
            interface::tauri::canvas_commands::canvas_node_create,
            interface::tauri::canvas_commands::canvas_node_update,
            interface::tauri::canvas_commands::canvas_node_delete,
            interface::tauri::canvas_commands::canvas_node_position,
            interface::tauri::canvas_commands::canvas_entity_positions,
            interface::tauri::canvas_commands::canvas_entity_position_save,
            interface::tauri::canvas_commands::canvas_layout_clear,
            interface::tauri::canvas_commands::canvas_edges,
            interface::tauri::canvas_commands::canvas_edge_create,
            interface::tauri::canvas_commands::canvas_edge_delete,
            interface::tauri::canvas_commands::attachments_list,
            interface::tauri::canvas_commands::attachment_create,
            interface::tauri::canvas_commands::attachment_delete,
            interface::tauri::collaboration_commands::collaboration_sessions,
            interface::tauri::collaboration_commands::collaboration_contributions,
            interface::tauri::collaboration_commands::collaboration_save_session,
            interface::tauri::collaboration_commands::collaboration_store_contribution,
            interface::tauri::collaboration_commands::collaboration_end_all,
            interface::tauri::collaboration_commands::collaboration_end_session,
            interface::tauri::collaboration_commands::collaboration_review,
            interface::tauri::collaboration_commands::collaboration_approve_all,
            interface::tauri::entity_commands::entity_list,
            interface::tauri::entity_commands::entity_details,
            interface::tauri::entity_commands::entity_create,
            interface::tauri::entity_commands::entity_update,
            interface::tauri::entity_commands::entity_delete,
            interface::tauri::entity_commands::entity_attribute_save,
            interface::tauri::entity_commands::entity_attribute_delete,
            interface::tauri::knowledge_commands::tags_list,
            interface::tauri::knowledge_commands::tags_for_owner,
            interface::tauri::knowledge_commands::tag_assignments,
            interface::tauri::knowledge_commands::tag_create,
            interface::tauri::knowledge_commands::tag_set,
            interface::tauri::knowledge_commands::tag_delete,
            interface::tauri::knowledge_commands::mentions_list,
            interface::tauri::knowledge_commands::mentions_sync,
            interface::tauri::manuscript_commands::story_list,
            interface::tauri::manuscript_commands::story_create,
            interface::tauri::manuscript_commands::story_update,
            interface::tauri::manuscript_commands::story_delete,
            interface::tauri::manuscript_commands::book_list_by_story,
            interface::tauri::manuscript_commands::book_list_by_universe,
            interface::tauri::manuscript_commands::book_create,
            interface::tauri::manuscript_commands::book_update,
            interface::tauri::manuscript_commands::book_delete,
            interface::tauri::manuscript_commands::chapter_list_by_book,
            interface::tauri::manuscript_commands::chapter_list_by_universe,
            interface::tauri::manuscript_commands::chapter_get,
            interface::tauri::manuscript_commands::chapter_create,
            interface::tauri::manuscript_commands::chapter_update,
            interface::tauri::manuscript_commands::chapter_reorder,
            interface::tauri::manuscript_commands::chapter_delete,
            interface::tauri::planning_commands::planning_list,
            interface::tauri::planning_commands::planning_create,
            interface::tauri::planning_commands::planning_delete,
            interface::tauri::planning_commands::planning_save_order,
            interface::tauri::planning_commands::planning_field_links,
            interface::tauri::planning_commands::planning_field_definitions,
            interface::tauri::planning_commands::planning_field_definition_create,
            interface::tauri::planning_commands::planning_field_definition_rename,
            interface::tauri::planning_commands::planning_field_definition_set_scope,
            interface::tauri::planning_commands::planning_field_definition_delete,
            updater_configured,
            database::health::database_health,
            database::health::database_compatibility,
            database::backup::backup_create,
            database::backup::backup_list,
            database::backup::backup_validate,
            database::recovery::backup_restore_prepare,
            database::recovery::backup_restore_commit,
            database::production_replica::production_replica_status,
            database::production_replica::production_replica_refresh,
            database::production_replica::production_replica_catalog,
            database::production_replica::production_replica_chapter,
            interface::tauri::planning_commands::planning_save_card,
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
