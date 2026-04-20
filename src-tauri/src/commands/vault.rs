use std::fs;
use std::path::PathBuf;

fn vault_base() -> PathBuf {
    dirs::home_dir()
        .expect("No home directory")
        .join(".ikrs-workspace")
        .join("vaults")
}

/// Resolve the vault path for a client slug, with basic slug
/// validation (no path separators, no traversal, no empty). Shared
/// by vault / notes_sync / future direct-filesystem commands.
pub fn vault_path_for_slug(slug: &str) -> Result<PathBuf, String> {
    if slug.is_empty()
        || slug.contains('/')
        || slug.contains('\\')
        || slug.contains("..")
    {
        return Err(format!("invalid client slug: {slug:?}"));
    }
    Ok(vault_base().join(slug))
}

#[tauri::command]
pub async fn create_vault(client_slug: String) -> Result<String, String> {
    let vault_path = vault_base().join(&client_slug);
    if vault_path.exists() {
        return Ok(vault_path.to_string_lossy().to_string());
    }

    let dirs_to_create = [
        ".obsidian",
        "00-inbox",
        "01-meetings",
        "02-tasks",
        "03-deliverables",
        "04-reference",
        "_templates",
    ];

    for dir in &dirs_to_create {
        fs::create_dir_all(vault_path.join(dir))
            .map_err(|e| format!("Failed to create vault dir {dir}: {e}"))?;
    }

    let templates = [
        ("_templates/meeting-note.md", "# Meeting: {{title}}\n\n**Date:** {{date}}\n**Attendees:** \n\n## Agenda\n\n## Notes\n\n## Action Items\n"),
        ("_templates/task-note.md", "# Task: {{title}}\n\n**Status:** \n**Priority:** \n\n## Context\n\n## Progress\n\n## Blockers\n"),
        ("_templates/daily-note.md", "# {{date}}\n\n## Focus\n\n## Log\n\n## Reflections\n"),
    ];

    for (path, content) in &templates {
        fs::write(vault_path.join(path), content)
            .map_err(|e| format!("Failed to write {path}: {e}"))?;
    }

    let readme = format!(
        "# {} — Engagement Vault\n\nCreated by IKAROS Workspace.\n",
        client_slug
    );
    fs::write(vault_path.join("README.md"), readme)
        .map_err(|e| format!("Failed to write README: {e}"))?;

    fs::write(
        vault_path.join(".obsidian/app.json"),
        r#"{"theme":"obsidian"}"#,
    )
    .map_err(|e| format!("Failed to write obsidian config: {e}"))?;

    Ok(vault_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn archive_vault(client_slug: String) -> Result<String, String> {
    let vault_path = vault_base().join(&client_slug);
    if !vault_path.exists() {
        return Err(format!("Vault not found: {client_slug}"));
    }

    let archive_dir = dirs::home_dir()
        .expect("No home directory")
        .join(".ikrs-workspace")
        .join("archive");
    fs::create_dir_all(&archive_dir).map_err(|e| e.to_string())?;

    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let archive_name = format!("{client_slug}-{date}.tar.gz");
    let archive_path = archive_dir.join(&archive_name);

    let tar_gz = fs::File::create(&archive_path).map_err(|e| e.to_string())?;
    let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);
    tar.append_dir_all(&client_slug, &vault_path)
        .map_err(|e| format!("Failed to archive: {e}"))?;
    tar.finish().map_err(|e| e.to_string())?;

    fs::remove_dir_all(&vault_path).map_err(|e| e.to_string())?;

    Ok(archive_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn restore_vault(archive_path: String) -> Result<String, String> {
    let archive = std::path::Path::new(&archive_path);
    if !archive.exists() {
        return Err(format!("Archive not found: {archive_path}"));
    }

    let tar_gz = fs::File::open(archive).map_err(|e| e.to_string())?;
    let dec = flate2::read::GzDecoder::new(tar_gz);
    let mut tar = tar::Archive::new(dec);
    tar.unpack(vault_base())
        .map_err(|e| format!("Failed to restore: {e}"))?;

    Ok("Vault restored".to_string())
}

#[tauri::command]
pub async fn delete_vault(client_slug: String) -> Result<(), String> {
    let vault_path = vault_base().join(&client_slug);
    if vault_path.exists() {
        fs::remove_dir_all(&vault_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
