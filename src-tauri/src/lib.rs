mod claude;
mod commands;
mod oauth;
mod skills;

use claude::ClaudeSessionManager;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_sql::Builder::new().build())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_keyring::init())
        .manage(commands::oauth::OAuthState::default())
        .manage(ClaudeSessionManager::new())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().expect("No app data dir");
            claude::registry::cleanup_orphans(&app_data_dir);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::credentials::store_credential,
            commands::credentials::get_credential,
            commands::credentials::delete_credential,
            commands::oauth::start_oauth,
            commands::oauth::exchange_oauth_code,
            commands::oauth::start_oauth_flow,
            commands::oauth::cancel_oauth_flow,
            commands::vault::create_vault,
            commands::vault::archive_vault,
            commands::vault::restore_vault,
            commands::vault::delete_vault,
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
