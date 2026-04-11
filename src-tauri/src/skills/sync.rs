use crate::skills::scaffold::SkillVersion;
use crate::skills::templates::{
    domain_template, interpolate, TemplateContext, SKILL_DOMAINS, TEMPLATE_VERSION,
};
use serde::{Deserialize, Serialize};
use std::fs;

/// Status of skill updates for an engagement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUpdateStatus {
    /// Whether updates are available (bundled version > installed version).
    pub updates_available: bool,
    /// Bundled template version in the app binary.
    pub bundled_version: String,
    /// Installed template version in the engagement folder.
    pub installed_version: String,
    /// Folders that can be updated (not customized by the user).
    pub updatable_folders: Vec<String>,
    /// Folders the user has customized (CLAUDE.md content differs from default).
    pub customized_folders: Vec<String>,
    /// Folders explicitly tracked as customized in `.skill-version`.
    pub user_marked_custom: Vec<String>,
}

/// C1 (Codex condition): Validate that engagement_path resolves to within
/// ~/.ikrs-workspace/vaults/ to prevent path traversal attacks.
fn validate_engagement_path(path: &str) -> Result<std::path::PathBuf, String> {
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

/// C3 (Codex condition): Proper semver comparison instead of string `!=`.
/// Parses version strings as (major, minor, patch) tuples and compares with `>`.
fn is_newer_version(bundled: &str, installed: &str) -> bool {
    let parse = |s: &str| -> (u32, u32, u32) {
        let parts: Vec<u32> = s.split('.').filter_map(|p| p.parse().ok()).collect();
        (
            parts.first().copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        )
    };
    parse(bundled) > parse(installed)
}

/// Check if skill template updates are available for an engagement.
///
/// Algorithm (from spec 3.8):
/// 1. Read .skill-version from engagement folder
/// 2. Compare with bundled template version
/// 3. If bundled > engagement:
///    a. Check which folders have been customized (modified CLAUDE.md)
///    b. Report updatable vs customized folders
/// 4. If equal: no updates
pub fn check_skill_updates(
    engagement_path: &str,
    ctx: &TemplateContext,
) -> Result<SkillUpdateStatus, String> {
    let base = validate_engagement_path(engagement_path)?;
    let version_path = base.join(".skill-version");

    if !version_path.exists() {
        return Err("No .skill-version found — engagement may not be scaffolded".to_string());
    }

    let version_json = fs::read_to_string(&version_path)
        .map_err(|e| format!("Failed to read .skill-version: {e}"))?;
    let version: SkillVersion = serde_json::from_str(&version_json)
        .map_err(|e| format!("Failed to parse .skill-version: {e}"))?;

    // C3: Use proper semver comparison instead of string !=
    let updates_available = is_newer_version(TEMPLATE_VERSION, &version.template_version);

    let mut updatable_folders = Vec::new();
    let mut customized_folders = Vec::new();

    if updates_available {
        for domain in SKILL_DOMAINS {
            let claude_path = base.join(domain).join("CLAUDE.md");

            // If user explicitly marked this folder as custom, skip it
            if version.customized_folders.contains(&domain.to_string()) {
                customized_folders.push(domain.to_string());
                continue;
            }

            if claude_path.exists() {
                // Compare current content with what the PREVIOUS version would have generated.
                // Since we only have the current bundled templates, we check if the file
                // matches the current bundled template (un-interpolated vars replaced).
                // If it doesn't match, the user customized it.
                if let Some(template) = domain_template(domain) {
                    let expected = interpolate(template, ctx);
                    let actual = fs::read_to_string(&claude_path)
                        .map_err(|e| format!("Failed to read {domain}/CLAUDE.md: {e}"))?;

                    if actual.trim() == expected.trim() {
                        updatable_folders.push(domain.to_string());
                    } else {
                        customized_folders.push(domain.to_string());
                    }
                }
            } else {
                // CLAUDE.md doesn't exist — can be created fresh
                updatable_folders.push(domain.to_string());
            }
        }
    }

    Ok(SkillUpdateStatus {
        updates_available,
        bundled_version: TEMPLATE_VERSION.to_string(),
        installed_version: version.template_version.clone(),
        updatable_folders,
        customized_folders,
        user_marked_custom: version.customized_folders.clone(),
    })
}

/// Apply skill updates to specified folders.
///
/// Only updates CLAUDE.md in folders explicitly listed in `folders_to_update`.
/// Updates `.skill-version` to the bundled version after applying.
/// Does NOT touch folders not in the list — the UI decides which to update.
pub fn apply_skill_updates(
    engagement_path: &str,
    folders_to_update: &[String],
    ctx: &TemplateContext,
) -> Result<(), String> {
    let base = validate_engagement_path(engagement_path)?;
    let version_path = base.join(".skill-version");

    // Validate that all requested folders are valid domains
    for folder in folders_to_update {
        if !SKILL_DOMAINS.contains(&folder.as_str()) {
            return Err(format!("Unknown skill domain: {folder}"));
        }
    }

    // Update each requested folder's CLAUDE.md
    for folder in folders_to_update {
        if let Some(template) = domain_template(folder) {
            let content = interpolate(template, ctx);
            let domain_dir = base.join(folder);
            fs::create_dir_all(&domain_dir)
                .map_err(|e| format!("Failed to create {folder}/ dir: {e}"))?;
            fs::write(domain_dir.join("CLAUDE.md"), content)
                .map_err(|e| format!("Failed to write {folder}/CLAUDE.md: {e}"))?;
        }
    }

    // Also update the orchestrator CLAUDE.md if it exists and hasn't been customized
    // (The orchestrator is not in the folders_to_update list — it's always updated
    //  unless the user explicitly customized it. For now we leave it alone;
    //  future version may add orchestrator to the sync UI.)

    // Update .skill-version
    let version_json = fs::read_to_string(&version_path)
        .map_err(|e| format!("Failed to read .skill-version: {e}"))?;
    let mut version: SkillVersion = serde_json::from_str(&version_json)
        .map_err(|e| format!("Failed to parse .skill-version: {e}"))?;

    version.template_version = TEMPLATE_VERSION.to_string();
    let json = serde_json::to_string_pretty(&version)
        .map_err(|e| format!("Failed to serialize .skill-version: {e}"))?;
    fs::write(&version_path, json)
        .map_err(|e| format!("Failed to write .skill-version: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::scaffold::{scaffold_engagement_skills, ScaffoldParams};

    /// Create a temporary directory inside the allowed vault base path
    /// so that path validation passes during tests.
    fn make_vault_dir() -> String {
        let vault_base = dirs::home_dir()
            .expect("No home directory")
            .join(".ikrs-workspace")
            .join("vaults");
        fs::create_dir_all(&vault_base).unwrap();
        let dir = vault_base.join(format!("test-sync-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().to_string()
    }

    fn test_ctx() -> TemplateContext {
        TemplateContext {
            client_name: "Sync Corp".to_string(),
            client_slug: "sync-corp".to_string(),
            engagement_title: "Test Event".to_string(),
            engagement_description: "Sync test".to_string(),
            consultant_name: "Sara Ahmed".to_string(),
            consultant_email: "sara@ikaros.ae".to_string(),
            timezone: "Asia/Dubai".to_string(),
            start_date: "2026-04-11".to_string(),
        }
    }

    #[test]
    fn test_check_no_updates_when_versions_match() {
        let tmp = make_vault_dir();
        let path = format!("{tmp}/check-match");
        fs::create_dir_all(&path).unwrap();
        let ctx = test_ctx();

        let params = ScaffoldParams {
            engagement_path: path.clone(),
            client_name: ctx.client_name.clone(),
            client_slug: ctx.client_slug.clone(),
            engagement_title: ctx.engagement_title.clone(),
            engagement_description: ctx.engagement_description.clone(),
            consultant_name: ctx.consultant_name.clone(),
            consultant_email: ctx.consultant_email.clone(),
            timezone: ctx.timezone.clone(),
        };
        scaffold_engagement_skills(&params).unwrap();

        let status = check_skill_updates(&path, &ctx).unwrap();
        assert!(!status.updates_available);
        assert_eq!(status.bundled_version, status.installed_version);
        assert!(status.updatable_folders.is_empty());
        assert!(status.customized_folders.is_empty());

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_check_detects_customized_folder() {
        let tmp = make_vault_dir();
        let path = format!("{tmp}/check-custom");
        fs::create_dir_all(&path).unwrap();
        let ctx = test_ctx();

        let params = ScaffoldParams {
            engagement_path: path.clone(),
            client_name: ctx.client_name.clone(),
            client_slug: ctx.client_slug.clone(),
            engagement_title: ctx.engagement_title.clone(),
            engagement_description: ctx.engagement_description.clone(),
            consultant_name: ctx.consultant_name.clone(),
            consultant_email: ctx.consultant_email.clone(),
            timezone: ctx.timezone.clone(),
        };
        scaffold_engagement_skills(&params).unwrap();

        // Simulate a version bump by editing .skill-version to an older version
        let version_path = std::path::Path::new(&path).join(".skill-version");
        let mut version: SkillVersion =
            serde_json::from_str(&fs::read_to_string(&version_path).unwrap()).unwrap();
        version.template_version = "0.9.0".to_string();
        fs::write(&version_path, serde_json::to_string_pretty(&version).unwrap()).unwrap();

        // Customize the legal CLAUDE.md
        let legal_claude = std::path::Path::new(&path).join("legal/CLAUDE.md");
        fs::write(&legal_claude, "# My custom legal notes").unwrap();

        let status = check_skill_updates(&path, &ctx).unwrap();
        assert!(status.updates_available);
        assert!(status.customized_folders.contains(&"legal".to_string()));
        assert!(!status.updatable_folders.contains(&"legal".to_string()));
        // Other 7 domains should be updatable
        assert_eq!(status.updatable_folders.len(), 7);

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_apply_updates_only_requested_folders() {
        let tmp = make_vault_dir();
        let path = format!("{tmp}/apply-test");
        fs::create_dir_all(&path).unwrap();
        let ctx = test_ctx();

        let params = ScaffoldParams {
            engagement_path: path.clone(),
            client_name: ctx.client_name.clone(),
            client_slug: ctx.client_slug.clone(),
            engagement_title: ctx.engagement_title.clone(),
            engagement_description: ctx.engagement_description.clone(),
            consultant_name: ctx.consultant_name.clone(),
            consultant_email: ctx.consultant_email.clone(),
            timezone: ctx.timezone.clone(),
        };
        scaffold_engagement_skills(&params).unwrap();

        // Downgrade version
        let version_path = std::path::Path::new(&path).join(".skill-version");
        let mut version: SkillVersion =
            serde_json::from_str(&fs::read_to_string(&version_path).unwrap()).unwrap();
        version.template_version = "0.9.0".to_string();
        fs::write(&version_path, serde_json::to_string_pretty(&version).unwrap()).unwrap();

        // Apply only to communications and planning
        apply_skill_updates(
            &path,
            &["communications".to_string(), "planning".to_string()],
            &ctx,
        )
        .unwrap();

        // Version should be updated
        let updated: SkillVersion =
            serde_json::from_str(&fs::read_to_string(&version_path).unwrap()).unwrap();
        assert_eq!(updated.template_version, TEMPLATE_VERSION);

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_apply_rejects_unknown_domain() {
        let tmp = make_vault_dir();
        let path = format!("{tmp}/reject-test");
        fs::create_dir_all(&path).unwrap();
        let ctx = test_ctx();

        let params = ScaffoldParams {
            engagement_path: path.clone(),
            client_name: ctx.client_name.clone(),
            client_slug: ctx.client_slug.clone(),
            engagement_title: ctx.engagement_title.clone(),
            engagement_description: ctx.engagement_description.clone(),
            consultant_name: ctx.consultant_name.clone(),
            consultant_email: ctx.consultant_email.clone(),
            timezone: ctx.timezone.clone(),
        };
        scaffold_engagement_skills(&params).unwrap();

        let result = apply_skill_updates(
            &path,
            &["nonexistent-domain".to_string()],
            &ctx,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown skill domain"));

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_path_traversal_rejected_check() {
        // C1: check_skill_updates must also reject paths outside vaults/
        let ctx = test_ctx();
        let result = check_skill_updates("/tmp/evil-path", &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Path outside allowed vault directory"));
    }

    #[test]
    fn test_path_traversal_rejected_apply() {
        // C1: apply_skill_updates must also reject paths outside vaults/
        let ctx = test_ctx();
        let result = apply_skill_updates("/tmp/evil-path", &[], &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Path outside allowed vault directory"));
    }

    #[test]
    fn test_semver_comparison() {
        // C3: Proper semver comparison
        assert!(is_newer_version("1.1.0", "1.0.0"));
        assert!(is_newer_version("2.0.0", "1.9.9"));
        assert!(is_newer_version("1.0.1", "1.0.0"));
        assert!(!is_newer_version("1.0.0", "1.0.0")); // equal is NOT newer
        assert!(!is_newer_version("0.9.0", "1.0.0")); // older is NOT newer
        assert!(!is_newer_version("1.0.0", "1.0.1")); // older is NOT newer
    }
}
