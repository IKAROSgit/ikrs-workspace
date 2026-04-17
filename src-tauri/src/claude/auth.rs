use crate::claude::binary_resolver::ResolvedBinaries;
use crate::claude::types::{AuthStatus, VersionCheck, MIN_CLAUDE_VERSION};
use std::process::Command;
use tauri::State;

/// Resolve the claude CLI path from the binary resolver, or return
/// the bare "claude" string if the resolver did not find it. The
/// bare fallback is fine for non-sandboxed dev builds where `PATH`
/// may be inherited from the shell; sandboxed packaged builds
/// depend on the resolver having located a concrete path at
/// startup (Phase 4a). Codex adversarial audit S12 flagged the
/// prior implementation's bare `Command::new("claude")` as
/// sandbox-unsafe.
fn claude_cmd(resolved: &ResolvedBinaries) -> Command {
    match resolved.claude.as_ref() {
        Some(path) => Command::new(path),
        None => Command::new("claude"),
    }
}

/// Check if Claude CLI is installed and meets minimum version.
#[tauri::command]
pub async fn claude_version_check(
    resolved: State<'_, ResolvedBinaries>,
) -> Result<VersionCheck, String> {
    // If the resolver found no claude, we are definitively NOT
    // installed under sandbox conditions regardless of what bare
    // `Command::new("claude")` might find via inherited PATH.
    if resolved.claude.is_none() {
        return Ok(VersionCheck {
            installed: false,
            version: None,
            meets_minimum: false,
        });
    }

    let output = claude_cmd(&resolved).arg("--version").output();

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
pub async fn claude_auth_status(
    resolved: State<'_, ResolvedBinaries>,
) -> Result<AuthStatus, String> {
    // Mirror the early-return posture from claude_version_check so
    // sandbox builds produce a coherent "not installed" signal rather
    // than a cryptic spawn error (Codex S12 follow-up, 2026-04-18).
    if resolved.claude.is_none() {
        return Ok(AuthStatus {
            logged_in: false,
            auth_method: None,
            api_provider: None,
        });
    }

    let output = claude_cmd(&resolved)
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
    serde_json::from_str(stdout.trim())
        .map_err(|e| format!("Failed to parse claude auth status: {e}"))
}

/// Initiate Claude CLI login (opens system browser for OAuth).
#[tauri::command]
pub async fn claude_auth_login(
    resolved: State<'_, ResolvedBinaries>,
) -> Result<(), String> {
    // Same symmetry as claude_auth_status — fail with a clear
    // installation error instead of a spawn error when the resolver
    // could not locate the binary (Codex S12 follow-up, 2026-04-18).
    if resolved.claude.is_none() {
        return Err(
            "Claude CLI is not installed or not on the resolved PATH. Install it and restart the app."
                .to_string(),
        );
    }

    let status = claude_cmd(&resolved)
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
        s.split('.').filter_map(|p| p.parse().ok()).collect()
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
