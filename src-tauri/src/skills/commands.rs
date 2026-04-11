use crate::skills::scaffold::{scaffold_engagement_skills, ScaffoldParams};
use crate::skills::sync::{apply_skill_updates, check_skill_updates, SkillUpdateStatus};
use crate::skills::templates::TemplateContext;

/// Scaffold skill folders for a new engagement.
///
/// Called from React when creating a new engagement.
/// Creates orchestrator CLAUDE.md + 8 domain folders + .skill-version.
#[tauri::command]
pub async fn scaffold_engagement_skills_cmd(
    engagement_path: String,
    client_name: String,
    client_slug: String,
    engagement_title: String,
    engagement_description: String,
    consultant_name: String,
    consultant_email: String,
    timezone: String,
) -> Result<String, String> {
    let params = ScaffoldParams {
        engagement_path,
        client_name,
        client_slug,
        engagement_title,
        engagement_description,
        consultant_name,
        consultant_email,
        timezone,
    };

    // Run filesystem operations on a blocking thread (not the async runtime)
    tokio::task::spawn_blocking(move || scaffold_engagement_skills(&params))
        .await
        .map_err(|e| format!("Scaffold task panicked: {e}"))?
}

/// Check if skill template updates are available for an engagement.
///
/// Called from React when opening an engagement (or from the skill status panel).
/// Returns which folders can be updated vs which have been customized.
#[tauri::command]
pub async fn check_skill_updates_cmd(
    engagement_path: String,
    client_name: String,
    client_slug: String,
    engagement_title: String,
    engagement_description: String,
    consultant_name: String,
    consultant_email: String,
    timezone: String,
    start_date: String,
) -> Result<SkillUpdateStatus, String> {
    let ctx = TemplateContext {
        client_name,
        client_slug,
        engagement_title,
        engagement_description,
        consultant_name,
        consultant_email,
        timezone,
        start_date,
    };

    let path = engagement_path.clone();
    tokio::task::spawn_blocking(move || check_skill_updates(&path, &ctx))
        .await
        .map_err(|e| format!("Check task panicked: {e}"))?
}

/// Apply skill template updates to selected folders.
///
/// Called from React when the user clicks "Update skills" on specific folders.
/// Only updates the folders listed — does not touch customized ones.
#[tauri::command]
pub async fn apply_skill_updates_cmd(
    engagement_path: String,
    folders_to_update: Vec<String>,
    client_name: String,
    client_slug: String,
    engagement_title: String,
    engagement_description: String,
    consultant_name: String,
    consultant_email: String,
    timezone: String,
    start_date: String,
) -> Result<(), String> {
    let ctx = TemplateContext {
        client_name,
        client_slug,
        engagement_title,
        engagement_description,
        consultant_name,
        consultant_email,
        timezone,
        start_date,
    };

    let path = engagement_path.clone();
    let folders = folders_to_update.clone();
    tokio::task::spawn_blocking(move || apply_skill_updates(&path, &folders, &ctx))
        .await
        .map_err(|e| format!("Apply task panicked: {e}"))?
}
