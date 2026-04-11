pub mod templates;
pub mod scaffold;
pub mod sync;
pub mod commands;

/// C1 (Codex condition): Validate that engagement_path resolves to within
/// ~/.ikrs-workspace/vaults/ to prevent path traversal attacks.
///
/// Shared by scaffold and sync modules (extracted per Codex I2 to prevent drift).
pub fn validate_engagement_path(path: &str) -> Result<std::path::PathBuf, String> {
    let p = std::path::PathBuf::from(path);
    let allowed_base = dirs::home_dir()
        .ok_or("No home directory")?
        .join(".ikrs-workspace")
        .join("vaults");
    std::fs::create_dir_all(&allowed_base).map_err(|e| e.to_string())?;
    let resolved = p.canonicalize().unwrap_or(p.clone());
    if !resolved.starts_with(&allowed_base) {
        return Err(format!("Path outside allowed vault directory: {}", path));
    }
    Ok(resolved)
}
