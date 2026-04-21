mod claude;
mod commands;
mod oauth;
mod skills;

use claude::ClaudeSessionManager;
use tauri::Manager;

/// One-time migration: move app data from old identifier directory to new.
fn migrate_app_data(app_data_dir: &std::path::Path) {
    let old_dir_name = "com.moe_ikaros_ae.ikrs-workspace";
    if let Some(parent) = app_data_dir.parent() {
        let old_dir = parent.join(old_dir_name);
        if old_dir.exists() && !app_data_dir.exists() {
            log::info!(
                "Migrating app data from {} to {}",
                old_dir.display(),
                app_data_dir.display()
            );
            if let Err(e) = std::fs::rename(&old_dir, app_data_dir) {
                log::warn!("Migration rename failed, trying file-by-file copy: {e}");
                if let Err(e2) = copy_dir_contents(&old_dir, app_data_dir) {
                    log::error!("Migration failed completely: {e2}");
                }
            }
        }
    }
}

fn copy_dir_contents(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let dest_path = dst.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_file() {
            std::fs::copy(entry.path(), dest_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_persisted_scope::init())
        .plugin(tauri_plugin_sql::Builder::new().build())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_keyring::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(commands::oauth::OAuthState::default())
        .manage(commands::task_watch::TaskWatchState::default())
        .manage(oauth::token_cache::TokenCache::default())
        .manage(ClaudeSessionManager::new())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().expect("No app data dir");
            migrate_app_data(&app_data_dir);

            // Resolve binary paths at startup (before sandbox restrictions)
            let resolved = claude::binary_resolver::resolve_binaries();
            if resolved.claude.is_none() {
                log::warn!("Claude CLI not found — sessions will fail to spawn");
            }
            if resolved.npx.is_none() {
                log::warn!("npx not found — MCP servers will be unavailable");
            }
            app.manage(resolved);

            claude::registry::cleanup_orphans(&app_data_dir);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::credentials::store_credential,
            commands::credentials::get_credential,
            commands::credentials::delete_credential,
            commands::oauth::start_oauth_flow,
            commands::oauth::cancel_oauth_flow,
            commands::oauth::start_firebase_identity_flow,
            commands::oauth::cancel_firebase_identity_flow,
            commands::oauth::clear_token_cache,
            commands::gmail_sync::list_gmail_inbox,
            commands::gmail_sync::send_gmail_message,
            commands::gmail_sync::mark_gmail_read,
            commands::calendar_sync::list_calendar_events,
            commands::calendar_sync::create_calendar_event,
            commands::drive_sync::list_drive_files,
            commands::notes_sync::list_vault_notes,
            commands::notes_sync::read_note_content,
            commands::task_watch::start_task_watch,
            commands::task_watch::stop_task_watch,
            commands::task_watch::write_task_frontmatter,
            commands::vault::create_vault,
            commands::vault::archive_vault,
            commands::vault::restore_vault,
            commands::vault::delete_vault,
            commands::vault::list_recent_vault_notes,
            // Claude M2 — embedded subprocess
            claude::auth::claude_version_check,
            claude::auth::claude_auth_status,
            claude::auth::claude_auth_login,
            claude::commands::spawn_claude_session,
            claude::commands::send_claude_message,
            claude::commands::kill_claude_session,
            claude::commands::get_resume_session_id,
            // Skills — Phase 2
            skills::commands::scaffold_engagement_skills_cmd,
            skills::commands::check_skill_updates_cmd,
            skills::commands::apply_skill_updates_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_migrate_app_data_moves_files() {
        let parent = std::env::temp_dir().join(format!("ikrs-mig-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&parent).unwrap();

        let old_dir = parent.join("com.moe_ikaros_ae.ikrs-workspace");
        fs::create_dir_all(&old_dir).unwrap();
        fs::write(old_dir.join("session-registry.json"), r#"{"sessions":{}}"#).unwrap();

        let new_dir = parent.join("ae.ikaros.workspace");
        migrate_app_data(&new_dir);

        assert!(new_dir.exists(), "new dir should exist after migration");
        assert!(
            new_dir.join("session-registry.json").exists(),
            "session-registry.json should be in new dir"
        );

        fs::remove_dir_all(&parent).ok();
    }

    #[test]
    fn test_migrate_app_data_skips_if_new_exists() {
        let parent = std::env::temp_dir().join(format!("ikrs-mig-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&parent).unwrap();

        let old_dir = parent.join("com.moe_ikaros_ae.ikrs-workspace");
        fs::create_dir_all(&old_dir).unwrap();
        fs::write(old_dir.join("old-file.txt"), "old").unwrap();

        let new_dir = parent.join("ae.ikaros.workspace");
        fs::create_dir_all(&new_dir).unwrap();
        fs::write(new_dir.join("new-file.txt"), "new").unwrap();

        migrate_app_data(&new_dir);

        // New dir should be untouched — old file should NOT appear
        assert!(!new_dir.join("old-file.txt").exists());
        assert!(new_dir.join("new-file.txt").exists());

        fs::remove_dir_all(&parent).ok();
    }

    #[test]
    fn test_copy_dir_contents() {
        let parent = std::env::temp_dir().join(format!("ikrs-copy-{}", uuid::Uuid::new_v4()));
        let src = parent.join("src");
        let dst = parent.join("dst");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), "hello").unwrap();
        fs::write(src.join("b.json"), "{}").unwrap();

        copy_dir_contents(&src, &dst).unwrap();

        assert_eq!(fs::read_to_string(dst.join("a.txt")).unwrap(), "hello");
        assert_eq!(fs::read_to_string(dst.join("b.json")).unwrap(), "{}");

        fs::remove_dir_all(&parent).ok();
    }
}
