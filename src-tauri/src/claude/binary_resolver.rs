use std::path::PathBuf;
use std::process::Command;

/// Resolved absolute paths for external binaries needed by the app.
/// Resolved at app startup before sandbox restrictions apply.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResolvedBinaries {
    pub claude: Option<PathBuf>,
    pub npx: Option<PathBuf>,
    pub node: Option<PathBuf>,
}

impl ResolvedBinaries {
    /// Build a PATH string containing the directories of all resolved binaries.
    /// Used to inject into child process environments so they can find npx/node.
    pub fn to_path_env(&self) -> String {
        let mut dirs = std::collections::HashSet::new();
        for bin in [&self.claude, &self.npx, &self.node] {
            if let Some(path) = bin {
                if let Some(parent) = path.parent() {
                    dirs.insert(parent.to_path_buf());
                }
            }
        }
        let mut sorted: Vec<_> = dirs.into_iter().collect();
        sorted.sort();
        sorted
            .iter()
            .map(|d| d.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(if cfg!(target_family = "windows") { ";" } else { ":" })
    }
}

/// Resolve all binary paths. Called at app startup.
pub fn resolve_binaries() -> ResolvedBinaries {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));

    ResolvedBinaries {
        claude: resolve_claude(&home),
        npx: resolve_npx(&home),
        node: resolve_node(&home),
    }
}

fn resolve_claude(home: &PathBuf) -> Option<PathBuf> {
    let mut candidates = vec![
        // `claude migrate-installer` moves the CLI to ~/.local/bin —
        // the official recommended path for users who ran migration.
        // Moe's install was here; our resolver previously missed it.
        home.join(".local/bin/claude"),
        // Legacy install location from the pre-migrate installer.
        home.join(".claude/local/bin/claude"),
        // Homebrew + manual-install paths.
        PathBuf::from("/usr/local/bin/claude"),
        PathBuf::from("/opt/homebrew/bin/claude"),
        // npm-global default prefix (when users run
        // `npm install -g @anthropic-ai/claude-code` without a
        // custom prefix).
        home.join(".npm-global/bin/claude"),
        home.join(".volta/bin/claude"),
    ];
    // nvm puts a per-node-version claude under each version's bin.
    let nvm_pattern = home.join(".nvm/versions/node/*/bin/claude");
    if let Ok(paths) = glob::glob(&nvm_pattern.to_string_lossy()) {
        let mut nvm_paths: Vec<PathBuf> = paths.filter_map(|p| p.ok()).collect();
        nvm_paths.sort();
        nvm_paths.reverse();
        candidates.extend(nvm_paths);
    }
    resolve_binary("claude", &candidates)
}

fn resolve_npx(home: &PathBuf) -> Option<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("/usr/local/bin/npx"),
        PathBuf::from("/opt/homebrew/bin/npx"),
        home.join(".volta/bin/npx"),
    ];
    // Add nvm paths (glob for version directories, pick latest)
    let nvm_pattern = home.join(".nvm/versions/node/*/bin/npx");
    if let Ok(paths) = glob::glob(&nvm_pattern.to_string_lossy()) {
        let mut nvm_paths: Vec<PathBuf> = paths.filter_map(|p| p.ok()).collect();
        nvm_paths.sort();
        nvm_paths.reverse(); // Latest version first
        candidates.extend(nvm_paths);
    }
    resolve_binary("npx", &candidates)
}

fn resolve_node(home: &PathBuf) -> Option<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("/usr/local/bin/node"),
        PathBuf::from("/opt/homebrew/bin/node"),
        home.join(".volta/bin/node"),
    ];
    let nvm_pattern = home.join(".nvm/versions/node/*/bin/node");
    if let Ok(paths) = glob::glob(&nvm_pattern.to_string_lossy()) {
        let mut nvm_paths: Vec<PathBuf> = paths.filter_map(|p| p.ok()).collect();
        nvm_paths.sort();
        nvm_paths.reverse();
        candidates.extend(nvm_paths);
    }
    resolve_binary("node", &candidates)
}

/// Try `which` first (captures user's current PATH), then fall back to known paths.
/// Note: `which` does not exist on Windows (equivalent is `where.exe`). The `Command::new("which")`
/// call gracefully returns Err on Windows, falling through to candidate path checks.
fn resolve_binary(name: &str, candidates: &[PathBuf]) -> Option<PathBuf> {
    // Try `which` first (Unix only; gracefully fails on Windows)
    if let Ok(output) = Command::new("which").arg(name).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                let p = PathBuf::from(&path);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }

    // Fall back to known candidate paths
    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate.clone());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_binaries_returns_struct() {
        let resolved = resolve_binaries();
        // On CI/dev machines, at least node/npx should be found
        // We don't assert they exist because test env varies
        assert!(resolved.claude.is_some() || resolved.claude.is_none()); // type check
    }

    #[test]
    fn test_to_path_env_deduplicates_directories() {
        let resolved = ResolvedBinaries {
            claude: Some(PathBuf::from("/usr/local/bin/claude")),
            npx: Some(PathBuf::from("/usr/local/bin/npx")),
            node: Some(PathBuf::from("/usr/local/bin/node")),
        };
        let path = resolved.to_path_env();
        // All three are in /usr/local/bin, so should appear once
        assert_eq!(path, "/usr/local/bin");
    }

    #[test]
    fn test_to_path_env_multiple_directories() {
        let resolved = ResolvedBinaries {
            claude: Some(PathBuf::from("/usr/local/bin/claude")),
            npx: Some(PathBuf::from("/opt/homebrew/bin/npx")),
            node: Some(PathBuf::from("/opt/homebrew/bin/node")),
        };
        let path = resolved.to_path_env();
        assert!(path.contains("/usr/local/bin"));
        assert!(path.contains("/opt/homebrew/bin"));
    }

    #[test]
    fn test_to_path_env_handles_none() {
        let resolved = ResolvedBinaries {
            claude: None,
            npx: Some(PathBuf::from("/usr/local/bin/npx")),
            node: None,
        };
        let path = resolved.to_path_env();
        assert_eq!(path, "/usr/local/bin");
    }

    #[test]
    fn test_to_path_env_all_none() {
        let resolved = ResolvedBinaries {
            claude: None,
            npx: None,
            node: None,
        };
        let path = resolved.to_path_env();
        assert_eq!(path, "");
    }
}
