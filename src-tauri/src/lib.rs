mod commands;
mod mcp;
mod oauth;

use mcp::manager::McpProcessManager;

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
        .manage(McpProcessManager::new())
        .invoke_handler(tauri::generate_handler![
            commands::credentials::store_credential,
            commands::credentials::get_credential,
            commands::credentials::delete_credential,
            commands::oauth::start_oauth,
            commands::oauth::exchange_oauth_code,
            commands::mcp::spawn_mcp,
            commands::mcp::kill_mcp,
            commands::mcp::kill_all_mcp,
            commands::mcp::mcp_health,
            commands::mcp::restart_mcp,
            commands::vault::create_vault,
            commands::vault::archive_vault,
            commands::vault::restore_vault,
            commands::vault::delete_vault,
            commands::claude::claude_preflight,
            commands::claude::scaffold_claude_project,
            commands::claude::launch_claude,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
