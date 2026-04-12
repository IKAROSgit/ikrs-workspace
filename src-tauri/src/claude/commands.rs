use crate::claude::session_manager::ClaudeSessionManager;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub async fn spawn_claude_session(
    engagement_id: String,
    engagement_path: String,
    resume_session_id: Option<String>,
    state: State<'_, ClaudeSessionManager>,
    app: AppHandle,
) -> Result<String, String> {
    let session_id = state
        .spawn(engagement_id.clone(), engagement_path, resume_session_id, app.clone())
        .await?;

    // Register in session registry for resume + orphan cleanup
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("No app data dir: {e}"))?;
    let _ = crate::claude::registry::register_session(
        &app_data_dir,
        &engagement_id,
        &session_id,
        std::process::id(),
    );

    Ok(session_id)
}

#[tauri::command]
pub async fn send_claude_message(
    session_id: String,
    message: String,
    state: State<'_, ClaudeSessionManager>,
) -> Result<(), String> {
    state.send_message(&session_id, &message).await
}

#[tauri::command]
pub async fn kill_claude_session(
    session_id: String,
    state: State<'_, ClaudeSessionManager>,
) -> Result<(), String> {
    state.kill(&session_id).await
}

#[tauri::command]
pub async fn get_resume_session_id(
    engagement_id: String,
    app: AppHandle,
) -> Result<Option<String>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("No app data dir: {e}"))?;
    Ok(crate::claude::registry::get_session_id(&app_data_dir, &engagement_id))
}
