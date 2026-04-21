//! Tauri command wrapper around the Phase 4B distiller.
//!
//! The frontend invokes this at session-end points (engagement
//! switch, explicit kill, app quit) with the transcript markdown.
//! The actual distill+merge runs async in a tokio task so the
//! session-end path doesn't block on the distiller's 10–90s
//! Claude CLI call.
//!
//! Returns `true` if the distiller produced a merge; `false` if it
//! was a no-op (disabled, empty transcript, no new entries). Errors
//! are surfaced to the caller only for argument validation — the
//! async distill itself logs its own outcome and cannot block the
//! UI.

use crate::claude::binary_resolver::ResolvedBinaries;
use crate::commands::vault::vault_path_for_slug;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub async fn distill_session_memory(
    client_slug: String,
    transcript: String,
    app: AppHandle,
    resolved: State<'_, ResolvedBinaries>,
) -> Result<bool, String> {
    // Cheap guards: if transcript is tiny, there's nothing to distill.
    if transcript.trim().len() < 200 {
        log::debug!(
            "distill_session_memory: transcript too short ({}B), skipping",
            transcript.len()
        );
        return Ok(false);
    }
    let vault = vault_path_for_slug(&client_slug)?;
    if !vault.exists() {
        return Err(format!("vault missing for slug {client_slug}"));
    }
    let claude_path = resolved
        .claude
        .clone()
        .ok_or_else(|| "Claude CLI not resolved".to_string())?;

    // Detach: run in the background. The session-end UI path doesn't
    // need to wait for a 60s Claude call to complete.
    let app_handle = app.clone();
    tokio::spawn(async move {
        match crate::memory::distill_and_persist(
            vault.clone(),
            transcript,
            claude_path,
        )
        .await
        {
            Ok(true) => {
                log::info!(
                    "distiller merged new memory entries for vault {}",
                    vault.display()
                );
                let _ = app_handle.emit("memory:updated", &client_slug);
            }
            Ok(false) => {
                log::info!(
                    "distiller produced no updates for vault {}",
                    vault.display()
                );
            }
            Err(e) => {
                log::warn!("distiller failed for vault {}: {e}", vault.display());
            }
        }
    });

    Ok(true)
}

// Bring Emitter into scope for the emit() call above.
#[allow(unused_imports)]
use tauri::Emitter;
