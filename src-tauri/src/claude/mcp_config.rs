use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct McpServerEntry {
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
}

#[derive(Serialize)]
struct McpConfig {
    #[serde(rename = "mcpServers")]
    mcp_servers: HashMap<String, McpServerEntry>,
}

/// Generates `.mcp-config.json` in the engagement directory.
/// Returns the absolute path to the generated config file.
///
/// - `google_access_token`: if Some, includes Gmail, Calendar, Drive servers
///   with the actual token value substituted into their env fields.
///   **The actual token is written to disk** rather than a
///   `${GOOGLE_ACCESS_TOKEN}` placeholder. Codex adversarial audit S14
///   flagged the placeholder approach as unverified against Claude CLI's
///   actual mcp-config parsing behaviour (Gate 6 violation — we could
///   not confirm whether Claude CLI does `${VAR}` interpolation from
///   its own env into MCP-child env). Writing the concrete value makes
///   the behaviour deterministic: what's in the file is what the MCP
///   subprocess receives. Security tradeoff: the token lives briefly
///   on disk. Mitigated by (a) file is in `.gitignore`, (b) lives under
///   the user's own engagement dir, (c) file perms 600 via
///   `apply_restrictive_perms`, (d) regenerated on every session spawn
///   (short lifetime, and the token itself is a 1-hour access token
///   that the refresh module swaps out).
/// - `vault_path`: if Some and directory exists, includes Obsidian server
pub fn generate_mcp_config(
    engagement_path: &Path,
    google_access_token: Option<&str>,
    vault_path: Option<&Path>,
    npx_path: Option<&Path>,
) -> Result<PathBuf, String> {
    let npx_command = npx_path
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "npx".to_string());

    let mut servers = HashMap::new();

    if let Some(token) = google_access_token {
        servers.insert(
            "gmail".to_string(),
            McpServerEntry {
                command: npx_command.clone(),
                args: vec!["@shinzolabs/gmail-mcp@1.7.4".to_string()],
                env: HashMap::from([(
                    "GOOGLE_ACCESS_TOKEN".to_string(),
                    token.to_string(),
                )]),
            },
        );
        servers.insert(
            "calendar".to_string(),
            McpServerEntry {
                command: npx_command.clone(),
                args: vec!["@cocal/google-calendar-mcp@2.6.1".to_string()],
                env: HashMap::from([(
                    "GOOGLE_ACCESS_TOKEN".to_string(),
                    token.to_string(),
                )]),
            },
        );
        servers.insert(
            "drive".to_string(),
            McpServerEntry {
                command: npx_command.clone(),
                args: vec!["@piotr-agier/google-drive-mcp@2.0.2".to_string()],
                env: HashMap::from([(
                    "GOOGLE_ACCESS_TOKEN".to_string(),
                    token.to_string(),
                )]),
            },
        );
    }

    if let Some(vp) = vault_path {
        if vp.exists() {
            servers.insert(
                "obsidian".to_string(),
                McpServerEntry {
                    command: npx_command.clone(),
                    args: vec![
                        "@bitbonsai/mcpvault@1.3.0".to_string(),
                        vp.to_string_lossy().to_string(),
                    ],
                    env: HashMap::new(),
                },
            );
        }
    }

    let config = McpConfig {
        mcp_servers: servers,
    };
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize MCP config: {e}"))?;

    let config_path = engagement_path.join(".mcp-config.json");
    // Atomic-600 write (Codex S14 follow-up 2026-04-18): create the
    // tmp file directly with mode 0o600 using OpenOptions so no
    // umask-default 0644 window exists between creation and chmod.
    // Then rename into place. On Windows this falls through to a
    // standard write — Windows ACLs on a user profile already
    // restrict to the owner by default.
    let tmp_path = engagement_path.join(".mcp-config.json.tmp");
    write_with_restrictive_perms(&tmp_path, json.as_bytes())?;
    std::fs::rename(&tmp_path, &config_path)
        .map_err(|e| format!("Failed to rename MCP config: {e}"))?;

    Ok(config_path)
}

/// Create a file with permissions 0o600 on Unix (owner rw, no group /
/// other access) from the moment it exists. Avoids the TOCTOU window
/// that `write` + chmod has — the file is never world-readable even
/// for microseconds.
#[cfg(unix)]
fn write_with_restrictive_perms(path: &Path, contents: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    // Best-effort removal in case a stale tmp lingers from a prior
    // crashed spawn; otherwise `create_new` below would refuse.
    let _ = std::fs::remove_file(path);

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("Failed to create MCP tmp file at 0600: {e}"))?;
    file.write_all(contents)
        .map_err(|e| format!("Failed to write MCP config: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("Failed to fsync MCP config: {e}"))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_with_restrictive_perms(path: &Path, contents: &[u8]) -> Result<(), String> {
    std::fs::write(path, contents)
        .map_err(|e| format!("Failed to write MCP config: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const FAKE_TOKEN: &str = "ya29.fake-access-token-for-tests";

    #[test]
    fn test_generate_config_with_google_token() {
        let dir = TempDir::new().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let result = generate_mcp_config(dir.path(), Some(FAKE_TOKEN), Some(&vault), None);
        assert!(result.is_ok());

        let config_path = result.unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let servers = parsed["mcpServers"].as_object().unwrap();
        assert!(servers.contains_key("gmail"));
        assert!(servers.contains_key("calendar"));
        assert!(servers.contains_key("drive"));
        assert!(servers.contains_key("obsidian"));
        assert_eq!(servers.len(), 4);
    }

    #[test]
    fn test_generate_config_token_written_literally() {
        // S14 core regression guard: the actual token value appears in
        // every Google-provider env, NOT the `${GOOGLE_ACCESS_TOKEN}`
        // placeholder. Codex-flagged fix 2026-04-17.
        let dir = TempDir::new().unwrap();
        let result = generate_mcp_config(dir.path(), Some(FAKE_TOKEN), None, None);
        assert!(result.is_ok());

        let content = fs::read_to_string(result.unwrap()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        for provider in ["gmail", "calendar", "drive"] {
            let env = &parsed["mcpServers"][provider]["env"]["GOOGLE_ACCESS_TOKEN"];
            assert_eq!(
                env.as_str().unwrap(),
                FAKE_TOKEN,
                "{provider} should contain literal token value, not placeholder"
            );
        }
        // Defensive: no placeholder string anywhere in the written file.
        assert!(!content.contains("${GOOGLE_ACCESS_TOKEN}"));
    }

    #[cfg(unix)]
    #[test]
    fn test_generate_config_file_perms_600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let result = generate_mcp_config(dir.path(), Some(FAKE_TOKEN), None, None);
        let config_path = result.unwrap();
        let mode = fs::metadata(&config_path).unwrap().permissions().mode();
        // On Unix the low 9 bits are rwxrwxrwx; 0o600 = rw-------.
        assert_eq!(mode & 0o777, 0o600, "config file must be 0600 on Unix");
    }

    #[test]
    fn test_generate_config_no_token() {
        let dir = TempDir::new().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let result = generate_mcp_config(dir.path(), None, Some(&vault), None);
        assert!(result.is_ok());

        let config_path = result.unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let servers = parsed["mcpServers"].as_object().unwrap();
        assert!(!servers.contains_key("gmail"));
        assert!(servers.contains_key("obsidian"));
        assert_eq!(servers.len(), 1);
    }

    #[test]
    fn test_generate_config_no_vault() {
        let dir = TempDir::new().unwrap();
        let nonexistent = dir.path().join("missing_vault");

        let result = generate_mcp_config(dir.path(), Some(FAKE_TOKEN), Some(&nonexistent), None);
        assert!(result.is_ok());

        let config_path = result.unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let servers = parsed["mcpServers"].as_object().unwrap();
        assert!(servers.contains_key("gmail"));
        assert!(!servers.contains_key("obsidian"));
        assert_eq!(servers.len(), 3);
    }

    #[test]
    fn test_generate_config_empty() {
        let dir = TempDir::new().unwrap();

        let result = generate_mcp_config(dir.path(), None, None, None);
        assert!(result.is_ok());

        let config_path = result.unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let servers = parsed["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 0);
    }
}
