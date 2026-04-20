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

    // 4. Write .claude/settings.local.json pre-approving safe,
    //    vault-scoped tools so consultants never see permission-prompt
    //    flashes that auto-dismiss in the UI.
    //
    //    Background (2026-04-20): the Claude Code CLI permission
    //    system prompts inline in the stream for every Write/Edit
    //    tool use that isn't pre-approved. Our chat UI rendered
    //    those prompts as transient toasts that disappeared before
    //    the consultant could click — making in-session file saves
    //    fail silently and forcing the consultant to drop to a
    //    terminal to hand-edit the settings file. Unacceptable UX
    //    for a per-client workspace product.
    //
    //    Scope rationale:
    //      - Write / Edit / NotebookEdit / Read / Glob / Grep run
    //        scoped to the engagement_path by Claude Code's cwd and
    //        the orchestrator CLAUDE.md conventions, so auto-allow
    //        is safe within this vault.
    //      - Bash stays DISALLOWED (enforced by session_manager's
    //        `--disallowed-tools Bash` flag; not even listed here).
    //      - MCP tools (mcp__gmail__*, mcp__drive__*, mcp__obsidian__*)
    //        aren't pre-approved here — their auth was already granted
    //        at Connect-Google time, and the in-chat "tool use" visible
    //        card is a useful audit trail for consultants.
    //
    //    Idempotent: existing settings preserved on re-scaffold.
    let claude_dir = base.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");
    if !settings_path.exists() {
        fs::create_dir_all(&claude_dir)
            .map_err(|e| format!("Failed to create .claude/ dir: {e}"))?;
        let settings = serde_json::json!({
            "permissions": {
                "allow": [
                    "Write",
                    "Edit",
                    "NotebookEdit",
                    "Read",
                    "Glob",
                    "Grep"
                ]
            }
        });
        let json = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Failed to serialize settings.local.json: {e}"))?;
        fs::write(&settings_path, json)
            .map_err(|e| format!("Failed to write settings.local.json: {e}"))?;
    }

    Ok(params.engagement_path.clone())
}

/// Backfill `.claude/settings.local.json` for engagements scaffolded
/// before 2026-04-20 (when this auto-write was added). Called at
/// session-spawn time so existing daily-use vaults pick up the
/// auto-allow permissions without requiring the user to re-scaffold.
///
/// Idempotent: no-op if the file already exists.
pub fn backfill_claude_settings(engagement_path: &std::path::Path) -> Result<(), String> {
    let settings_path = engagement_path.join(".claude/settings.local.json");
    if settings_path.exists() {
        return Ok(());
    }
    let claude_dir = engagement_path.join(".claude");
    fs::create_dir_all(&claude_dir)
        .map_err(|e| format!("backfill: create .claude: {e}"))?;
    let settings = serde_json::json!({
        "permissions": {
            "allow": [
                "Write",
                "Edit",
                "NotebookEdit",
                "Read",
                "Glob",
                "Grep"
            ]
        }
    });
    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("backfill: serialize: {e}"))?;
    fs::write(&settings_path, json)
        .map_err(|e| format!("backfill: write: {e}"))?;
    Ok(())
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
    fn test_scaffold_writes_claude_settings_with_safe_allow_list() {
        // Regression for the 2026-04-20 permissions UX fix. Every
        // freshly scaffolded engagement MUST ship with a
        // `.claude/settings.local.json` that pre-allows Write / Edit
        // / NotebookEdit / Read / Glob / Grep — otherwise Claude's
        // inline permission prompts flash and auto-dismiss in the
        // chat UI, forcing consultants to edit config files in a
        // terminal. Also verify Bash is NOT on the allow list (it's
        // disallowed at the CLI-flag level and must never leak in).
        let tmp = make_vault_dir();
        let engagement_path = format!("{tmp}/settings-test");
        fs::create_dir_all(&engagement_path).unwrap();

        let params = ScaffoldParams {
            engagement_path: engagement_path.clone(),
            client_name: "Perm Corp".to_string(),
            client_slug: "perm-corp".to_string(),
            engagement_title: "E".to_string(),
            engagement_description: "E".to_string(),
            consultant_name: "U".to_string(),
            consultant_email: "u@u.com".to_string(),
            timezone: "UTC".to_string(),
        };
        scaffold_engagement_skills(&params).unwrap();

        let settings_path = std::path::Path::new(&engagement_path)
            .join(".claude/settings.local.json");
        assert!(settings_path.exists(), ".claude/settings.local.json missing");

        let content = fs::read_to_string(&settings_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let allow = parsed["permissions"]["allow"]
            .as_array()
            .expect("allow list missing")
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>();

        for required in ["Write", "Edit", "NotebookEdit", "Read", "Glob", "Grep"] {
            assert!(
                allow.contains(&required.to_string()),
                "settings.local.json missing '{required}' in allow list — consultants will see auto-dismissing permission prompts"
            );
        }
        assert!(
            !allow.iter().any(|s| s == "Bash"),
            "settings.local.json must NEVER auto-allow Bash"
        );

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_backfill_claude_settings_creates_when_missing() {
        // Backfill path for engagements scaffolded before 2026-04-20.
        let tmp = make_vault_dir();
        let engagement_path = std::path::Path::new(&tmp).join("legacy-engagement");
        fs::create_dir_all(&engagement_path).unwrap();

        assert!(!engagement_path.join(".claude/settings.local.json").exists());
        backfill_claude_settings(&engagement_path).unwrap();
        assert!(engagement_path.join(".claude/settings.local.json").exists());

        let content = fs::read_to_string(
            engagement_path.join(".claude/settings.local.json"),
        )
        .unwrap();
        assert!(content.contains("\"Write\""));
        assert!(!content.contains("\"Bash\""));

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_backfill_is_idempotent_and_preserves_custom_settings() {
        // Backfill must NEVER overwrite a user-customised settings
        // file. If a consultant has added project-specific allows
        // (or, more importantly, denies), we keep theirs as-is.
        let tmp = make_vault_dir();
        let engagement_path = std::path::Path::new(&tmp).join("custom-settings");
        fs::create_dir_all(engagement_path.join(".claude")).unwrap();
        let custom = r#"{"permissions":{"allow":["Read"],"deny":["Write"]}}"#;
        fs::write(
            engagement_path.join(".claude/settings.local.json"),
            custom,
        )
        .unwrap();

        backfill_claude_settings(&engagement_path).unwrap();

        let after = fs::read_to_string(
            engagement_path.join(".claude/settings.local.json"),
        )
        .unwrap();
        assert_eq!(after, custom, "backfill clobbered user's custom settings");

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
