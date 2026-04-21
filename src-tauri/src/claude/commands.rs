use crate::claude::session_manager::ClaudeSessionManager;
use crate::commands::credentials::make_keychain_key;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub async fn spawn_claude_session(
    engagement_id: String,
    engagement_path: String,
    resume_session_id: Option<String>,
    client_slug: Option<String>,
    strict_mcp: Option<bool>,
    state: State<'_, ClaudeSessionManager>,
    app: AppHandle,
) -> Result<String, String> {
    let mut env_vars = std::collections::HashMap::new();
    let mut mcp_config_path: Option<String> = None;

    // 1. Read Google OAuth payload from keychain, refresh if expired.
    //    We need the FULL payload (not just access_token) because the
    //    gmail MCP spawn consumes CLIENT_ID/CLIENT_SECRET/REFRESH_TOKEN
    //    — see mcp_config::GoogleOAuthCreds docstring for rationale.
    let keychain_key = make_keychain_key(&engagement_id, "google");
    let google_payload =
        crate::oauth::token_refresh::get_payload_refresh_if_needed(&keychain_key, &app)
            .await
            .ok();
    let has_token = google_payload.is_some();

    // Strict MCP: require Google token for fresh spawns (skip on resume -- Codex I2)
    if resume_session_id.is_none() && strict_mcp.unwrap_or(false) && !has_token {
        return Err("Strict MCP mode: Google authentication required. Please authenticate before starting this session.".to_string());
    }

    // Keep GOOGLE_ACCESS_TOKEN in the env_vars for the Claude CLI's
    // own process. S14 fix means the MCP config now embeds the actual
    // token directly rather than relying on `${VAR}` interpolation —
    // env_vars here is defence-in-depth: if any future MCP adds its
    // own env dependency or if Claude CLI itself ever reads the env.
    if let Some(ref payload) = google_payload {
        env_vars.insert(
            "GOOGLE_ACCESS_TOKEN".to_string(),
            payload.access_token.clone(),
        );
    }

    // 2. Resolve vault path and ensure directory exists (Codex C1 fix)
    //    Only if client_slug is provided (engagements without clients skip MCP)
    if let Some(ref slug) = client_slug {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let vault_path = std::path::PathBuf::from(&home)
            .join(".ikrs-workspace")
            .join("vaults")
            .join(slug);
        if !vault_path.exists() {
            if let Err(e) = std::fs::create_dir_all(&vault_path) {
                log::warn!("Failed to create vault dir {}: {e}", vault_path.display());
            }
        }

        // 2a. Backfill .claude/settings.local.json for pre-2026-04-20
        //     vaults so Claude's Write/Edit/NotebookEdit tool use
        //     doesn't trigger permission prompts that auto-dismiss in
        //     the UI. Safe to run every spawn — idempotent.
        if let Err(e) = crate::skills::scaffold::backfill_claude_settings(&vault_path) {
            log::warn!("Claude settings backfill failed for {slug}: {e}");
        }

        // 2b. Phase 4C scaffold: ensure the new folders (daily-notes/,
        //     05-decisions/, 06-briefs/, _memory/) + templates +
        //     engagement CLAUDE.md + today's daily note exist. Runs
        //     on every spawn so pre-Phase-4C vaults migrate
        //     transparently. Idempotent: existing files are left
        //     alone, only gaps are filled.
        if let Err(e) = crate::commands::vault::scaffold_vault_idempotent(
            &vault_path,
            slug,
        ) {
            log::warn!("Vault scaffold failed for {slug}: {e}");
        }
        match crate::commands::vault::ensure_today_daily_note(&vault_path) {
            Ok(Some(created)) => log::info!(
                "Created today's daily note: {}",
                created.display(),
            ),
            Ok(None) => {}
            Err(e) => log::warn!("Daily-note ensure failed for {slug}: {e}"),
        }

        // 3. Generate MCP config (with resolved npx path for sandbox compatibility)
        //    S14 fix: pass the actual token value, not a placeholder.
        //    Gmail-MCP (2026-04-18) consumes the full OAuth credential
        //    set rather than the bare access_token — see
        //    mcp_config::GoogleOAuthCreds docstring.
        let resolved: tauri::State<'_, crate::claude::binary_resolver::ResolvedBinaries> =
            app.state();
        let engagement_dir = std::path::Path::new(&engagement_path);
        let creds = google_payload.as_ref().map(|p| {
            crate::claude::mcp_config::GoogleOAuthCreds {
                access_token: &p.access_token,
                client_id: &p.client_id,
                client_secret: &p.client_secret,
                refresh_token: &p.refresh_token,
            }
        });
        let config_path = crate::claude::mcp_config::generate_mcp_config(
            engagement_dir,
            creds.as_ref(),
            Some(&vault_path),
            resolved.npx.as_deref(),
        )?;
        mcp_config_path = Some(config_path.to_string_lossy().to_string());

    }

    // 4. Spawn Claude with MCP config
    let (session_id, child_pid) = state
        .spawn(
            engagement_id.clone(),
            engagement_path,
            resume_session_id,
            env_vars,
            mcp_config_path,
            app.clone(),
        )
        .await?;

    // 5. Register in session registry for resume + orphan cleanup
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("No app data dir: {e}"))?;
    let _ = crate::claude::registry::register_session(
        &app_data_dir,
        &engagement_id,
        &session_id,
        child_pid,
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
    app: AppHandle,
) -> Result<(), String> {
    let engagement_id = state.kill(&session_id).await?;
    // Unregister from file registry so stale entries don't cause resume attempts
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        let _ = crate::claude::registry::unregister_session(&app_data_dir, &engagement_id);
    }
    Ok(())
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
