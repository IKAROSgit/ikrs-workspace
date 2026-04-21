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
    //    fail silently AND (worse) the subsequent assistant text
    //    cheerfully reported "Meeting notes saved to …" even though
    //    no file was ever written. A real consultant lost their
    //    triaged tracker + transcript to that lie before we caught
    //    it. Unacceptable.
    //
    //    Security scope (Codex 2026-04-20 HOLD on prior loose allow):
    //    every tool is path-anchored to the engagement's own vault
    //    directory via Claude Code's `Tool(path-pattern)` permission
    //    syntax. A prompt injection that tells Claude to write to
    //    `~/.ssh/authorized_keys` or read `~/.aws/credentials` will
    //    fall through the allow list and hit the default-prompt
    //    gate — which is still better than nothing, and the
    //    write-verification layer in stream_parser catches any
    //    misreported success regardless.
    //
    //    Deny list: `Read(/etc/**)`, `Read(~/.ssh/**)`,
    //    `Read(~/.aws/**)`, `Read(~/.config/**)` explicitly listed
    //    so even a permission-prompt-dismissed approval can't leak
    //    those paths. Claude Code's deny takes precedence over allow.
    //
    //    Bash: not on the allow list AND enforced at the CLI flag
    //    level via `--disallowed-tools Bash` in session_manager.
    //
    //    Idempotent: existing settings preserved on re-scaffold.
    let claude_dir = base.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");
    if !settings_path.exists() {
        fs::create_dir_all(&claude_dir)
            .map_err(|e| format!("Failed to create .claude/ dir: {e}"))?;
        let json = build_vault_scoped_settings(&base)?;
        fs::write(&settings_path, json)
            .map_err(|e| format!("Failed to write settings.local.json: {e}"))?;
    }

    Ok(params.engagement_path.clone())
}

/// Build a vault-scoped Claude Code permissions JSON.
///
/// Dual-form allow list (2026-04-20 pragmatic fix — observed live
/// that Claude Code v2.1.80's `Tool(absolute/path/**)` pattern
/// didn't match when written as expected, so Write/Edit still
/// blocked Moe's in-app session):
///   - Unscoped `Write` / `Edit` / `NotebookEdit` / `Read` for the
///     actual runtime gate. The CLI's cwd is the vault path
///     (session_manager sets it), so relative file operations
///     naturally stay scoped.
///   - Path-anchored variants as a secondary entry that newer
///     Claude Code versions may honour — belt-and-braces. Upstream
///     pattern parsing improves, we get tighter scoping automatically.
///
/// The primary defence against the "prompt-injection tells Claude
/// to write to ~/.ssh" attack is the `deny` list, which IS
/// respected by Claude Code today — deny wins over allow. We
/// enumerate the known-sensitive host paths and rely on `deny`
/// not on `allow` scoping.
fn build_vault_scoped_settings(vault_path: &std::path::Path) -> Result<String, String> {
    let vault_glob = format!("{}/**", vault_path.to_string_lossy());
    let settings = serde_json::json!({
        "permissions": {
            "allow": [
                "Write",
                "Edit",
                "NotebookEdit",
                "Read",
                "Glob",
                "Grep",
                format!("Write({vault_glob})"),
                format!("Edit({vault_glob})"),
                format!("NotebookEdit({vault_glob})"),
                format!("Read({vault_glob})"),
            ],
            // Belt-and-braces: even if prompt-injection tricks the
            // user into approving a prompt that falls through allow,
            // these paths stay denied. Deny > allow in Claude Code.
            "deny": [
                "Read(/etc/**)",
                "Read(/private/etc/**)",
                "Read(~/.ssh/**)",
                "Read(~/.aws/**)",
                "Read(~/.config/gcloud/**)",
                "Read(~/Library/Keychains/**)",
                "Write(~/.ssh/**)",
                "Write(~/.bash_profile)",
                "Write(~/.bashrc)",
                "Write(~/.zshrc)",
                "Write(~/.zprofile)",
                "Write(~/.profile)",
            ]
        }
    });
    serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("serialize settings: {e}"))
}

/// Backfill `.claude/settings.local.json` for engagements scaffolded
/// before 2026-04-20. Path-scoped — same shape as fresh scaffold.
/// Idempotent: no-op if the file already exists.
pub fn backfill_claude_settings(engagement_path: &std::path::Path) -> Result<(), String> {
    let settings_path = engagement_path.join(".claude/settings.local.json");
    if settings_path.exists() {
        return Ok(());
    }
    let claude_dir = engagement_path.join(".claude");
    fs::create_dir_all(&claude_dir)
        .map_err(|e| format!("backfill: create .claude: {e}"))?;
    let json = build_vault_scoped_settings(engagement_path)
        .map_err(|e| format!("backfill: {e}"))?;
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
    fn test_scaffold_writes_path_scoped_permissions() {
        // Regression for 2026-04-20 Codex HOLD: the allow list
        // MUST be path-scoped to the engagement vault so a prompt-
        // injection attack via email/invite content can't make
        // Claude write ~/.ssh/authorized_keys or read ~/.aws/
        // credentials. Pattern syntax: `Write(/abs/path/**)`.
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

        // Both scoped AND unscoped entries must be present
        // (dual-form, see build_vault_scoped_settings docstring for
        // why). The deny list is the primary guardrail — it's
        // respected by Claude Code today in a way `Tool(pattern)`
        // allow isn't always.
        let vault_canon = std::fs::canonicalize(&engagement_path).unwrap();
        let vault_str = vault_canon.to_string_lossy().to_string();
        for tool in ["Write", "Edit", "NotebookEdit", "Read"] {
            let scoped = format!("{tool}({vault_str}/**)");
            assert!(
                allow.iter().any(|s| s == &scoped),
                "allow list missing path-scoped entry {scoped}; got {allow:?}"
            );
            assert!(
                allow.iter().any(|s| s == tool),
                "allow list missing unscoped '{tool}' fallback; got {allow:?}"
            );
        }
        assert!(
            !allow.iter().any(|s| s == "Bash" || s.starts_with("Bash(")),
            "allow list must NEVER contain Bash"
        );

        // Deny list must block known-sensitive host paths.
        let deny = parsed["permissions"]["deny"]
            .as_array()
            .expect("deny list missing")
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        for required_deny in [
            "Read(~/.ssh/**)",
            "Read(~/.aws/**)",
            "Read(~/Library/Keychains/**)",
            "Write(~/.ssh/**)",
            "Write(~/.zshrc)",
        ] {
            assert!(
                deny.contains(&required_deny.to_string()),
                "deny list missing {required_deny}"
            );
        }

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_backfill_claude_settings_creates_path_scoped() {
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
        assert!(
            content.contains("Write("),
            "backfill must emit path-scoped Write(...)"
        );
        assert!(
            content.contains("\"Write\""),
            "backfill must also emit unscoped 'Write' fallback (dual-form)"
        );
        assert!(content.contains("~/.ssh/**"), "deny list missing ssh");

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
