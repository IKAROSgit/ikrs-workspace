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
/// - `has_google_token`: if true, includes Gmail, Calendar, Drive servers
/// - `vault_path`: if Some and directory exists, includes Obsidian server
pub fn generate_mcp_config(
    engagement_path: &Path,
    has_google_token: bool,
    vault_path: Option<&Path>,
) -> Result<PathBuf, String> {
    let mut servers = HashMap::new();

    if has_google_token {
        servers.insert(
            "gmail".to_string(),
            McpServerEntry {
                command: "npx".to_string(),
                args: vec!["@shinzolabs/gmail-mcp@1.7.4".to_string()],
                env: HashMap::from([(
                    "GOOGLE_ACCESS_TOKEN".to_string(),
                    "${GOOGLE_ACCESS_TOKEN}".to_string(),
                )]),
            },
        );
        servers.insert(
            "calendar".to_string(),
            McpServerEntry {
                command: "npx".to_string(),
                args: vec!["@cocal/google-calendar-mcp@2.6.1".to_string()],
                env: HashMap::from([(
                    "GOOGLE_ACCESS_TOKEN".to_string(),
                    "${GOOGLE_ACCESS_TOKEN}".to_string(),
                )]),
            },
        );
        servers.insert(
            "drive".to_string(),
            McpServerEntry {
                command: "npx".to_string(),
                args: vec!["@piotr-agier/google-drive-mcp@2.0.2".to_string()],
                env: HashMap::from([(
                    "GOOGLE_ACCESS_TOKEN".to_string(),
                    "${GOOGLE_ACCESS_TOKEN}".to_string(),
                )]),
            },
        );
    }

    if let Some(vp) = vault_path {
        if vp.exists() {
            servers.insert(
                "obsidian".to_string(),
                McpServerEntry {
                    command: "npx".to_string(),
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
    // Atomic write: tmp then rename
    let tmp_path = engagement_path.join(".mcp-config.json.tmp");
    std::fs::write(&tmp_path, &json)
        .map_err(|e| format!("Failed to write MCP config: {e}"))?;
    std::fs::rename(&tmp_path, &config_path)
        .map_err(|e| format!("Failed to rename MCP config: {e}"))?;

    Ok(config_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_generate_config_with_google_token() {
        let dir = TempDir::new().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let result = generate_mcp_config(dir.path(), true, Some(&vault));
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
    fn test_generate_config_no_token() {
        let dir = TempDir::new().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let result = generate_mcp_config(dir.path(), false, Some(&vault));
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

        let result = generate_mcp_config(dir.path(), true, Some(&nonexistent));
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

        let result = generate_mcp_config(dir.path(), false, None);
        assert!(result.is_ok());

        let config_path = result.unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let servers = parsed["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 0);
    }
}
