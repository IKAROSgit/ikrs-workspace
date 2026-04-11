use crate::claude::types::{AuthStatus, VersionCheck, MIN_CLAUDE_VERSION};
use std::process::Command;

/// Check if Claude CLI is installed and meets minimum version.
#[tauri::command]
pub async fn claude_version_check() -> Result<VersionCheck, String> {
    let output = Command::new("claude")
        .arg("--version")
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
            // Version string is like "2.1.92 (Claude Code)" — extract semver
            let semver = version.split_whitespace().next().unwrap_or("").to_string();
            let meets_minimum = compare_versions(&semver, MIN_CLAUDE_VERSION);
            Ok(VersionCheck {
                installed: true,
                version: Some(semver),
                meets_minimum,
            })
        }
        _ => Ok(VersionCheck {
            installed: false,
            version: None,
            meets_minimum: false,
        }),
    }
}

/// Check Claude CLI authentication status.
#[tauri::command]
pub async fn claude_auth_status() -> Result<AuthStatus, String> {
    let output = Command::new("claude")
        .args(["auth", "status"])
        .output()
        .map_err(|e| format!("Failed to check claude auth: {e}"))?;

    if !output.status.success() {
        return Ok(AuthStatus {
            logged_in: false,
            auth_method: None,
            api_provider: None,
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).map_err(|e| {
        format!("Failed to parse claude auth status: {e}")
    })
}

/// Initiate Claude CLI login (opens system browser for OAuth).
#[tauri::command]
pub async fn claude_auth_login() -> Result<(), String> {
    let status = Command::new("claude")
        .args(["auth", "login"])
        .status()
        .map_err(|e| format!("Failed to start claude auth login: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Claude auth login failed".to_string())
    }
}

/// Simple semver comparison: returns true if `version` >= `minimum`.
fn compare_versions(version: &str, minimum: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let v = parse(version);
    let m = parse(minimum);

    for i in 0..3 {
        let a = v.get(i).copied().unwrap_or(0);
        let b = m.get(i).copied().unwrap_or(0);
        if a > b {
            return true;
        }
        if a < b {
            return false;
        }
    }
    true // Equal
}
