use crate::skills::templates::{
    domain_subfolders, domain_template, interpolate, TemplateContext,
    ORCHESTRATOR_TEMPLATE, SKILL_DOMAINS, TEMPLATE_VERSION,
};
use serde::{Deserialize, Serialize};
use std::fs;

/// The `.skill-version` file content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersion {
    pub template_version: String,
    pub scaffolded_at: String,
    pub customized_folders: Vec<String>,
}

/// Parameters for scaffolding engagement skills.
/// These map to the TemplateContext fields.
#[derive(Debug, Deserialize)]
pub struct ScaffoldParams {
    pub engagement_path: String,
    pub client_name: String,
    pub client_slug: String,
    pub engagement_title: String,
    pub engagement_description: String,
    pub consultant_name: String,
    pub consultant_email: String,
    pub timezone: String,
}

use super::validate_engagement_path;

/// Scaffold all skill folders and CLAUDE.md files for an engagement.
///
/// Creates:
/// - `{engagement_path}/CLAUDE.md` (orchestrator)
/// - `{engagement_path}/{domain}/CLAUDE.md` for each of 8 domains
/// - `{engagement_path}/{domain}/{subfolder}/` for each domain's subfolders
/// - `{engagement_path}/.skill-version`
///
/// Idempotent: skips files/dirs that already exist.
pub fn scaffold_engagement_skills(params: &ScaffoldParams) -> Result<String, String> {
    let base = validate_engagement_path(&params.engagement_path)?;

    if !base.exists() {
        fs::create_dir_all(&base)
            .map_err(|e| format!("Failed to create engagement dir: {e}"))?;
    }

    let now = chrono::Utc::now().to_rfc3339();
    let ctx = TemplateContext {
        client_name: params.client_name.clone(),
        client_slug: params.client_slug.clone(),
        engagement_title: params.engagement_title.clone(),
        engagement_description: params.engagement_description.clone(),
        consultant_name: params.consultant_name.clone(),
        consultant_email: params.consultant_email.clone(),
        timezone: params.timezone.clone(),
        start_date: now.split('T').next().unwrap_or(&now).to_string(),
    };

    // 1. Write orchestrator CLAUDE.md at root
    let orchestrator_path = base.join("CLAUDE.md");
    if !orchestrator_path.exists() {
        let content = interpolate(ORCHESTRATOR_TEMPLATE, &ctx);
        fs::write(&orchestrator_path, content)
            .map_err(|e| format!("Failed to write orchestrator CLAUDE.md: {e}"))?;
    }

    // 2. Create each domain folder, its CLAUDE.md, and subfolders
    for domain in SKILL_DOMAINS {
        let domain_dir = base.join(domain);
        fs::create_dir_all(&domain_dir)
            .map_err(|e| format!("Failed to create {domain}/ dir: {e}"))?;

        // Domain CLAUDE.md
        let claude_path = domain_dir.join("CLAUDE.md");
        if !claude_path.exists() {
            if let Some(template) = domain_template(domain) {
                let content = interpolate(template, &ctx);
                fs::write(&claude_path, content)
                    .map_err(|e| format!("Failed to write {domain}/CLAUDE.md: {e}"))?;
            }
        }

        // Subfolders
        for subfolder in domain_subfolders(domain) {
            let sub_path = domain_dir.join(subfolder);
            if !sub_path.exists() {
                fs::create_dir_all(&sub_path)
                    .map_err(|e| format!("Failed to create {domain}/{subfolder}/: {e}"))?;
            }
        }
    }

    // 3. Write .skill-version
    let version_path = base.join(".skill-version");
    if !version_path.exists() {
        let version = SkillVersion {
            template_version: TEMPLATE_VERSION.to_string(),
            scaffolded_at: now,
            customized_folders: vec![],
        };
        let json = serde_json::to_string_pretty(&version)
            .map_err(|e| format!("Failed to serialize .skill-version: {e}"))?;
        fs::write(&version_path, json)
            .map_err(|e| format!("Failed to write .skill-version: {e}"))?;
    }

    Ok(params.engagement_path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a temporary directory inside the allowed vault base path
    /// so that path validation passes during tests.
    fn make_vault_dir() -> String {
        let vault_base = dirs::home_dir()
            .expect("No home directory")
            .join(".ikrs-workspace")
            .join("vaults");
        fs::create_dir_all(&vault_base).unwrap();
        let dir = vault_base.join(format!("test-scaffold-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().to_string()
    }

    #[test]
    fn test_scaffold_creates_all_files() {
        let tmp = make_vault_dir();
        let engagement_path = format!("{tmp}/test-engagement");
        // Pre-create so canonicalize works
        fs::create_dir_all(&engagement_path).unwrap();

        let params = ScaffoldParams {
            engagement_path: engagement_path.clone(),
            client_name: "Test Corp".to_string(),
            client_slug: "test-corp".to_string(),
            engagement_title: "Annual Event".to_string(),
            engagement_description: "Test event".to_string(),
            consultant_name: "Sara Ahmed".to_string(),
            consultant_email: "sara@ikaros.ae".to_string(),
            timezone: "Asia/Dubai".to_string(),
        };

        let result = scaffold_engagement_skills(&params);
        assert!(result.is_ok());

        // Orchestrator CLAUDE.md exists
        let orchestrator = std::path::Path::new(&engagement_path).join("CLAUDE.md");
        assert!(orchestrator.exists(), "Missing orchestrator CLAUDE.md");
        let content = fs::read_to_string(&orchestrator).unwrap();
        assert!(content.contains("Test Corp"), "Orchestrator missing client name");
        assert!(content.contains("Sara Ahmed"), "Orchestrator missing consultant name");
        assert!(content.contains("Quality Gates"), "Orchestrator missing quality gates");

        // All 8 domain folders exist with CLAUDE.md
        for domain in SKILL_DOMAINS {
            let domain_claude = std::path::Path::new(&engagement_path)
                .join(domain)
                .join("CLAUDE.md");
            assert!(
                domain_claude.exists(),
                "Missing {domain}/CLAUDE.md"
            );
        }

        // .skill-version exists and is valid JSON
        let version_path = std::path::Path::new(&engagement_path).join(".skill-version");
        assert!(version_path.exists(), "Missing .skill-version");
        let version_json = fs::read_to_string(&version_path).unwrap();
        let version: SkillVersion = serde_json::from_str(&version_json).unwrap();
        assert_eq!(version.template_version, TEMPLATE_VERSION);
        assert!(version.customized_folders.is_empty());

        // Spot-check subfolders
        assert!(std::path::Path::new(&engagement_path).join("communications/meetings").exists());
        assert!(std::path::Path::new(&engagement_path).join("planning/timelines").exists());
        assert!(std::path::Path::new(&engagement_path).join("finance/invoices").exists());
        assert!(std::path::Path::new(&engagement_path).join("talent/riders").exists());

        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_scaffold_is_idempotent() {
        let tmp = make_vault_dir();
        let engagement_path = format!("{tmp}/idempotent-test");
        fs::create_dir_all(&engagement_path).unwrap();

        let params = ScaffoldParams {
            engagement_path: engagement_path.clone(),
            client_name: "Idem Corp".to_string(),
            client_slug: "idem-corp".to_string(),
            engagement_title: "Event".to_string(),
            engagement_description: "Test".to_string(),
            consultant_name: "Test User".to_string(),
            consultant_email: "test@test.com".to_string(),
            timezone: "UTC".to_string(),
        };

        // Run twice
        scaffold_engagement_skills(&params).unwrap();

        // Modify orchestrator CLAUDE.md (simulating user customization)
        let orchestrator = std::path::Path::new(&engagement_path).join("CLAUDE.md");
        fs::write(&orchestrator, "# Custom content").unwrap();

        // Run again — should NOT overwrite customized file
        scaffold_engagement_skills(&params).unwrap();
        let content = fs::read_to_string(&orchestrator).unwrap();
        assert_eq!(content, "# Custom content", "Scaffold overwrote existing file!");

        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_path_traversal_rejected() {
        // C1 (Codex condition): paths outside vaults/ must be rejected
        let params = ScaffoldParams {
            engagement_path: "/tmp/evil-traversal-test".to_string(),
            client_name: "Hacker".to_string(),
            client_slug: "hacker".to_string(),
            engagement_title: "Exploit".to_string(),
            engagement_description: "Path traversal attempt".to_string(),
            consultant_name: "Attacker".to_string(),
            consultant_email: "evil@example.com".to_string(),
            timezone: "UTC".to_string(),
        };

        let result = scaffold_engagement_skills(&params);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("Path outside allowed vault directory"),
            "Should reject paths outside vaults/"
        );
    }
}
