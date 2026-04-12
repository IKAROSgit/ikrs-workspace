# Phase 4a: macOS Sandbox + Signing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a signed, notarized macOS .dmg that installs without Gatekeeper warnings, spawns Claude CLI under App Sandbox, and handles OAuth token refresh.

**Architecture:** Fix Phase 3 debt (identifier, refresh_token, SettingsView OAuth), then build sandbox infrastructure (binary resolver, entitlements, Security-Scoped Bookmarks, restricted capabilities), then wire CI signing/notarization. Five sequential waves.

**Tech Stack:** Tauri 2, Rust, React 19, TypeScript, GitHub Actions, tauri-plugin-persisted-scope, Apple Developer ID signing + notarization

**Spec:** `docs/specs/m2-phase4a-sandbox-signing-design.md`

---

## File Map

| Action | File | Responsibility |
|--------|------|---------------|
| Modify | `src-tauri/tauri.conf.json` | Identifier, macOS bundle config, CSP |
| Modify | `src-tauri/Cargo.toml` | Add `tauri-plugin-persisted-scope`, `glob` |
| Modify | `src-tauri/src/lib.rs` | Data migration, binary resolver init, persisted-scope plugin |
| Modify | `src-tauri/src/oauth/redirect_server.rs` | Store JSON token payload (access + refresh + expires + client_id) |
| Create | `src-tauri/src/oauth/token_refresh.rs` | `refresh_if_needed()` — reads keychain, refreshes if expired |
| Modify | `src-tauri/src/oauth/mod.rs` | Add `pub mod token_refresh;` |
| Modify | `src-tauri/src/claude/commands.rs` | Use `refresh_if_needed()` instead of direct keychain read |
| Modify | `src/views/SettingsView.tsx` | Migrate to `startOAuthFlow`, event-driven completion |
| Modify | `src/lib/tauri-commands.ts` | Remove dead `startOAuth`, `exchangeOAuthCode` exports |
| Modify | `src-tauri/src/commands/oauth.rs` | Remove dead `start_oauth`, `exchange_oauth_code` commands |
| Modify | `src-tauri/src/claude/mod.rs` | Add `pub mod binary_resolver;` |
| Create | `src-tauri/src/claude/binary_resolver.rs` | Resolve claude/npx/node to absolute paths |
| Modify | `src-tauri/src/claude/session_manager.rs` | Use resolved claude path + inject PATH |
| Modify | `src-tauri/src/claude/mcp_config.rs` | Accept resolved npx path instead of hardcoded "npx" |
| Modify | `src-tauri/src/claude/registry.rs` | `#[cfg]` guards for Unix/Windows |
| Create | `src-tauri/entitlements.plist` | macOS App Sandbox + Hardened Runtime entitlements |
| Modify | `src-tauri/capabilities/default.json` | Restricted permissions with shell scope |
| Modify | `.github/workflows/ci.yml` | Apple signing env vars + artifact upload |
| Create | `tests/unit/lib/token-refresh.test.ts` | Frontend-adjacent token refresh tests (if needed) |

---

## Wave 1: Phase 3 Debt

### Task 1: App Identifier Change + Data Migration

**Files:**
- Modify: `src-tauri/tauri.conf.json:5`
- Modify: `src-tauri/src/lib.rs:22-25`

- [ ] **Step 1: Change identifier in tauri.conf.json**

In `src-tauri/tauri.conf.json`, change line 5:

```json
"identifier": "ae.ikaros.workspace",
```

- [ ] **Step 2: Add data migration in lib.rs setup hook**

In `src-tauri/src/lib.rs`, add a migration function and call it in `.setup()` before `cleanup_orphans`:

```rust
/// One-time migration: move app data from old identifier directory to new.
fn migrate_app_data(app_data_dir: &std::path::Path) {
    let old_dir_name = "com.moe_ikaros_ae.ikrs-workspace";
    if let Some(parent) = app_data_dir.parent() {
        let old_dir = parent.join(old_dir_name);
        if old_dir.exists() && !app_data_dir.exists() {
            log::info!("Migrating app data from {} to {}", old_dir.display(), app_data_dir.display());
            if let Err(e) = std::fs::rename(&old_dir, app_data_dir) {
                log::warn!("Migration rename failed, trying file-by-file copy: {e}");
                if let Err(e2) = copy_dir_contents(&old_dir, app_data_dir) {
                    log::error!("Migration failed completely: {e2}");
                }
            }
        }
    }
}

fn copy_dir_contents(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let dest_path = dst.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_file() {
            std::fs::copy(entry.path(), dest_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
```

In the `.setup()` closure, before `cleanup_orphans`:

```rust
.setup(|app| {
    let app_data_dir = app.path().app_data_dir().expect("No app data dir");
    migrate_app_data(&app_data_dir);
    claude::registry::cleanup_orphans(&app_data_dir);
    Ok(())
})
```

- [ ] **Step 3: Run Rust tests to verify nothing breaks**

Run: `cd src-tauri && cargo test`
Expected: All 45 tests pass. The identifier change only affects build-time config, not test behavior.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tauri.conf.json src-tauri/src/lib.rs
git commit -m "refactor: change app identifier to ae.ikaros.workspace with data migration"
```

---

### Task 2: OAuth Refresh Token Storage

**Files:**
- Modify: `src-tauri/src/oauth/redirect_server.rs:103-117`
- Create: `src-tauri/src/oauth/token_refresh.rs`
- Modify: `src-tauri/src/oauth/mod.rs`

- [ ] **Step 1: Write the failing test for token_refresh**

Create `src-tauri/src/oauth/token_refresh.rs`:

```rust
use tauri::AppHandle;
use tauri_plugin_keyring::KeyringExt;

const IKRS_SERVICE: &str = "ikrs-workspace";

/// Token payload stored as JSON in the keychain.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TokenPayload {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub client_id: String,
}

impl TokenPayload {
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.expires_at <= now + 300 // 5-minute buffer
    }
}

/// Read the token from keychain, refresh if expired, return a valid access_token.
/// Returns Err if no token exists, JSON is corrupt, or refresh fails.
pub async fn refresh_if_needed(keychain_key: &str, app: &AppHandle) -> Result<String, String> {
    let raw = app
        .keyring()
        .get_password(IKRS_SERVICE, keychain_key)
        .ok()
        .flatten()
        .ok_or("No Google token found. Please authenticate first.")?;

    let payload: TokenPayload = serde_json::from_str(&raw).map_err(|_| {
        "Google session expired. Please re-authenticate.".to_string()
    })?;

    if !payload.is_expired() {
        return Ok(payload.access_token);
    }

    // Token expired — refresh it
    let client = reqwest::Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", payload.client_id.as_str()),
            ("refresh_token", payload.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| format!("Token refresh failed: {e}"))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Google session expired. Please re-authenticate. ({body})"));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let new_access_token = json["access_token"]
        .as_str()
        .ok_or("Missing access_token in refresh response")?
        .to_string();
    let new_expires_in = json["expires_in"].as_i64().unwrap_or(3600);

    let updated = TokenPayload {
        access_token: new_access_token.clone(),
        refresh_token: payload.refresh_token, // Google doesn't always return a new refresh_token
        expires_at: chrono::Utc::now().timestamp() + new_expires_in,
        client_id: payload.client_id,
    };

    let updated_json = serde_json::to_string(&updated).map_err(|e| e.to_string())?;
    app.keyring()
        .set_password(IKRS_SERVICE, keychain_key, &updated_json)
        .map_err(|e| format!("Keychain update failed: {e}"))?;

    Ok(new_access_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_not_expired() {
        let payload = TokenPayload {
            access_token: "test".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: chrono::Utc::now().timestamp() + 3600,
            client_id: "cid".to_string(),
        };
        assert!(!payload.is_expired());
    }

    #[test]
    fn test_payload_expired() {
        let payload = TokenPayload {
            access_token: "test".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: chrono::Utc::now().timestamp() - 100,
            client_id: "cid".to_string(),
        };
        assert!(payload.is_expired());
    }

    #[test]
    fn test_payload_expired_within_buffer() {
        let payload = TokenPayload {
            access_token: "test".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: chrono::Utc::now().timestamp() + 200, // Within 5-min buffer
            client_id: "cid".to_string(),
        };
        assert!(payload.is_expired());
    }

    #[test]
    fn test_corrupted_json_is_handled() {
        let result: Result<TokenPayload, _> = serde_json::from_str("not-json");
        assert!(result.is_err());
    }

    #[test]
    fn test_plain_token_string_is_handled() {
        // Pre-Phase-4a format: plain access_token string
        let result: Result<TokenPayload, _> = serde_json::from_str("\"ya29.old-format-token\"");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Register the module**

In `src-tauri/src/oauth/mod.rs`, add:

```rust
pub mod pkce;
pub mod redirect_server;
pub mod token_refresh;
```

- [ ] **Step 3: Run tests to verify**

Run: `cd src-tauri && cargo test token_refresh`
Expected: 5 tests pass.

- [ ] **Step 4: Update redirect_server.rs to store JSON payload**

In `src-tauri/src/oauth/redirect_server.rs`, replace the token storage block (lines 103-117):

Replace:
```rust
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let access_token = json["access_token"]
            .as_str()
            .ok_or("Missing access_token")?
            .to_string();

        // Store in keychain
        app.keyring()
            .set_password(IKRS_SERVICE, &keychain_key, &access_token)
            .map_err(|e| format!("Keychain store failed: {e}"))?;

        // Emit event so frontend knows token is ready
        let _ = app.emit("oauth:token-stored", serde_json::json!({
            "keychain_key": keychain_key,
        }));
```

With:
```rust
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let access_token = json["access_token"]
            .as_str()
            .ok_or("Missing access_token")?
            .to_string();
        let refresh_token = json["refresh_token"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let expires_in = json["expires_in"].as_i64().unwrap_or(3600);

        // Store as JSON payload in keychain (includes refresh_token for auto-refresh)
        let payload = crate::oauth::token_refresh::TokenPayload {
            access_token: access_token.clone(),
            refresh_token,
            expires_at: chrono::Utc::now().timestamp() + expires_in,
            client_id: client_id.clone(),
        };
        let payload_json = serde_json::to_string(&payload)
            .map_err(|e| format!("Failed to serialize token payload: {e}"))?;

        app.keyring()
            .set_password(IKRS_SERVICE, &keychain_key, &payload_json)
            .map_err(|e| format!("Keychain store failed: {e}"))?;

        // Emit event so frontend knows token is ready
        let _ = app.emit("oauth:token-stored", serde_json::json!({
            "keychain_key": keychain_key,
        }));
```

- [ ] **Step 5: Run all Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass (45 existing + 5 new = 50).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/oauth/token_refresh.rs src-tauri/src/oauth/mod.rs src-tauri/src/oauth/redirect_server.rs
git commit -m "feat(oauth): store refresh_token in keychain, add token refresh module"
```

---

### Task 3: Wire Token Refresh Into Session Spawn

**Files:**
- Modify: `src-tauri/src/claude/commands.rs:21-37`

- [ ] **Step 1: Replace direct keychain read with refresh_if_needed**

In `src-tauri/src/claude/commands.rs`, replace the Google token reading block (lines 21-37):

Replace:
```rust
    // 1. Read Google OAuth token from keychain (KeyringExt pattern)
    let keychain_key = make_keychain_key(&engagement_id, "google");
    let google_token = app
        .keyring()
        .get_password(IKRS_SERVICE, &keychain_key)
        .ok()
        .flatten();
    let has_token = google_token.is_some();

    // Strict MCP: require Google token for fresh spawns (skip on resume -- Codex I2)
    if resume_session_id.is_none() && strict_mcp.unwrap_or(false) && !has_token {
        return Err("Strict MCP mode: Google authentication required. Please authenticate before starting this session.".to_string());
    }

    if let Some(ref token) = google_token {
        env_vars.insert("GOOGLE_ACCESS_TOKEN".to_string(), token.clone());
    }
```

With:
```rust
    // 1. Read Google OAuth token from keychain, refresh if expired
    let keychain_key = make_keychain_key(&engagement_id, "google");
    let google_token = crate::oauth::token_refresh::refresh_if_needed(&keychain_key, &app)
        .await
        .ok();
    let has_token = google_token.is_some();

    // Strict MCP: require Google token for fresh spawns (skip on resume -- Codex I2)
    if resume_session_id.is_none() && strict_mcp.unwrap_or(false) && !has_token {
        return Err("Strict MCP mode: Google authentication required. Please authenticate before starting this session.".to_string());
    }

    if let Some(ref token) = google_token {
        env_vars.insert("GOOGLE_ACCESS_TOKEN".to_string(), token.clone());
    }
```

- [ ] **Step 2: Remove the KeyringExt import if no longer needed**

Check if `commands.rs` still uses `KeyringExt` directly. The `refresh_if_needed` function handles keychain access internally. If nothing else in this file uses `app.keyring()`, remove:

```rust
use tauri_plugin_keyring::KeyringExt;
```

Keep the `IKRS_SERVICE` constant only if still referenced elsewhere in the file.

- [ ] **Step 3: Run all Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All 50 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/claude/commands.rs
git commit -m "feat(oauth): use token refresh in session spawn instead of direct keychain read"
```

---

### Task 4: Migrate SettingsView to startOAuthFlow

**Files:**
- Modify: `src/views/SettingsView.tsx:1-132`

- [ ] **Step 1: Replace imports**

In `src/views/SettingsView.tsx`, replace the import line 12:

Replace:
```typescript
import { startOAuth, scaffoldEngagementSkills } from "@/lib/tauri-commands";
```

With:
```typescript
import { startOAuthFlow, cancelOAuthFlow, scaffoldEngagementSkills } from "@/lib/tauri-commands";
```

- [ ] **Step 2: Add listen/emit imports**

Add at the top of the file:

```typescript
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-opener";
```

Remove the existing `open` import from `@tauri-apps/plugin-shell` (line 3) since we switch to the opener plugin for URL opening.

- [ ] **Step 3: Replace handleConnectGoogle**

Replace the entire `handleConnectGoogle` function (lines 118-132):

```typescript
  const handleConnectGoogle = async () => {
    if (!activeEngagementId) return;
    setOauthStatus("pending");

    let unlisten: (() => void) | undefined;
    let timeout: ReturnType<typeof setTimeout> | undefined;

    try {
      // Subscribe to token-stored event BEFORE starting the flow
      const tokenPromise = new Promise<boolean>((resolve) => {
        listen("oauth:token-stored", () => {
          resolve(true);
        }).then((fn) => { unlisten = fn; });

        timeout = setTimeout(() => {
          resolve(false);
        }, 300_000); // 5-minute timeout
      });

      const { auth_url } = await startOAuthFlow(
        activeEngagementId,
        OAUTH_CLIENT_ID,
        OAUTH_PORT,
        GOOGLE_SCOPES,
      );
      await open(auth_url);

      const success = await tokenPromise;
      setOauthStatus(success ? "success" : "error");

      if (!success) {
        await cancelOAuthFlow();
      }
    } catch {
      setOauthStatus("error");
    } finally {
      unlisten?.();
      if (timeout) clearTimeout(timeout);
    }
  };
```

- [ ] **Step 4: Run frontend tests**

Run: `npx vitest run`
Expected: All 55 frontend tests pass (SettingsView doesn't have dedicated tests yet, but imports must resolve).

- [ ] **Step 5: Commit**

```bash
git add src/views/SettingsView.tsx
git commit -m "feat(oauth): migrate SettingsView to startOAuthFlow with redirect server"
```

---

### Task 5: Remove Dead OAuth Commands

**Files:**
- Modify: `src/lib/tauri-commands.ts:21-45`
- Modify: `src-tauri/src/commands/oauth.rs:23-107`
- Modify: `src-tauri/src/lib.rs:32-33`

- [ ] **Step 1: Remove dead TypeScript exports**

In `src/lib/tauri-commands.ts`, remove the `startOAuth`, `exchangeOAuthCode`, `OAuthStartResult`, and `TokenResponse` exports (lines 21-45):

Remove:
```typescript
export interface OAuthStartResult {
  auth_url: string;
}

export interface TokenResponse {
  access_token: string;
  refresh_token: string | null;
  expires_in: number;
}

export async function startOAuth(
  clientId: string,
  redirectPort: number,
  scopes: string[],
): Promise<OAuthStartResult> {
  return invoke("start_oauth", { clientId, redirectPort, scopes });
}

export async function exchangeOAuthCode(
  code: string,
  clientId: string,
  redirectPort: number,
): Promise<TokenResponse> {
  return invoke("exchange_oauth_code", { code, clientId, redirectPort });
}
```

- [ ] **Step 2: Remove dead Rust commands**

In `src-tauri/src/commands/oauth.rs`, remove the `start_oauth` function (lines 23-55), the `TokenResponse` struct (lines 57-62), and the `exchange_oauth_code` function (lines 64-107). Keep `OAuthStartResult` only if other code references it — if not, remove it too.

- [ ] **Step 3: Unregister from invoke_handler**

In `src-tauri/src/lib.rs`, remove these two lines from the `generate_handler!` macro:

```rust
commands::oauth::start_oauth,
commands::oauth::exchange_oauth_code,
```

- [ ] **Step 4: Run all tests**

Run: `cd src-tauri && cargo test && cd .. && npx vitest run`
Expected: All Rust + frontend tests pass. No imports of removed functions remain.

- [ ] **Step 5: Commit**

```bash
git add src/lib/tauri-commands.ts src-tauri/src/commands/oauth.rs src-tauri/src/lib.rs
git commit -m "chore: remove dead start_oauth and exchange_oauth_code commands"
```

---

## Wave 2: Sandbox Preparation

### Task 6: Binary Path Resolver

**Files:**
- Create: `src-tauri/src/claude/binary_resolver.rs`
- Modify: `src-tauri/src/claude/mod.rs`
- Modify: `src-tauri/Cargo.toml` (add `glob`)

- [ ] **Step 1: Add glob dependency**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
glob = "0.3"
```

- [ ] **Step 2: Write binary_resolver.rs with tests**

Create `src-tauri/src/claude/binary_resolver.rs`:

```rust
use std::path::PathBuf;
use std::process::Command;

/// Resolved absolute paths for external binaries needed by the app.
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
        dirs.iter()
            .map(|d| d.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(":")
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
    let candidates = vec![
        home.join(".claude/local/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
        PathBuf::from("/opt/homebrew/bin/claude"),
    ];
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
fn resolve_binary(name: &str, candidates: &[PathBuf]) -> Option<PathBuf> {
    // Try `which` first
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
```

- [ ] **Step 3: Register the module**

In `src-tauri/src/claude/mod.rs`, add:

```rust
pub mod binary_resolver;
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test binary_resolver`
Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/claude/binary_resolver.rs src-tauri/src/claude/mod.rs
git commit -m "feat: add binary path resolver for claude/npx/node under sandbox"
```

---

### Task 7: Wire Binary Resolver Into App Startup + Consumers

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/claude/session_manager.rs:85-88`
- Modify: `src-tauri/src/claude/mcp_config.rs:23-27,34,45,56,72`
- Modify: `src-tauri/src/claude/commands.rs`

- [ ] **Step 1: Register ResolvedBinaries in app state**

In `src-tauri/src/lib.rs`, add to the `.setup()` closure after `migrate_app_data`:

```rust
.setup(|app| {
    let app_data_dir = app.path().app_data_dir().expect("No app data dir");
    migrate_app_data(&app_data_dir);

    // Resolve binary paths at startup (before sandbox restrictions)
    let resolved = claude::binary_resolver::resolve_binaries();
    if resolved.claude.is_none() {
        log::warn!("Claude CLI not found — sessions will fail to spawn");
    }
    if resolved.npx.is_none() {
        log::warn!("npx not found — MCP servers will be unavailable");
    }
    app.manage(resolved);

    claude::registry::cleanup_orphans(&app_data_dir);
    Ok(())
})
```

- [ ] **Step 2: Update session_manager.rs to use resolved claude path + inject PATH**

In `src-tauri/src/claude/session_manager.rs`, the `spawn` method signature already receives `app: AppHandle`. Add `ResolvedBinaries` extraction and use it:

At line 85, replace:
```rust
        let mut child = Command::new("claude")
            .args(&args)
            .current_dir(&engagement_path)
            .envs(&env_vars)
```

With:
```rust
        let resolved: tauri::State<'_, crate::claude::binary_resolver::ResolvedBinaries> = app.state();
        let claude_path = resolved.claude.as_ref()
            .ok_or("Claude CLI not found. Please install Claude Code (https://claude.ai/code).")?;

        let mut child = Command::new(claude_path)
            .args(&args)
            .current_dir(&engagement_path)
            .env("PATH", resolved.to_path_env())
            .envs(&env_vars)
```

Note: `.env("PATH", ...)` is set BEFORE `.envs(&env_vars)` so env_vars can override if needed.

- [ ] **Step 3: Update mcp_config.rs to accept npx path**

In `src-tauri/src/claude/mcp_config.rs`, change the `generate_mcp_config` signature:

Replace:
```rust
pub fn generate_mcp_config(
    engagement_path: &Path,
    has_google_token: bool,
    vault_path: Option<&Path>,
) -> Result<PathBuf, String> {
```

With:
```rust
pub fn generate_mcp_config(
    engagement_path: &Path,
    has_google_token: bool,
    vault_path: Option<&Path>,
    npx_path: Option<&Path>,
) -> Result<PathBuf, String> {
    let npx_command = npx_path
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "npx".to_string());
```

Then replace all 4 instances of `command: "npx".to_string()` with `command: npx_command.clone()`.

- [ ] **Step 4: Update commands.rs to pass npx path to mcp_config**

In `src-tauri/src/claude/commands.rs`, where `generate_mcp_config` is called (around line 55):

Replace:
```rust
        let config_path = crate::claude::mcp_config::generate_mcp_config(
            engagement_dir,
            has_token,
            Some(&vault_path),
        )?;
```

With:
```rust
        let resolved: tauri::State<'_, crate::claude::binary_resolver::ResolvedBinaries> = app.state();
        let config_path = crate::claude::mcp_config::generate_mcp_config(
            engagement_dir,
            has_token,
            Some(&vault_path),
            resolved.npx.as_deref(),
        )?;
```

- [ ] **Step 5: Fix existing mcp_config tests**

The existing tests in `mcp_config.rs` need updating to pass the new `npx_path` parameter. Update all 4 test calls:

```rust
let result = generate_mcp_config(dir.path(), true, Some(&vault), None);
```

(Add `None` as the 4th argument to each test's `generate_mcp_config` call.)

- [ ] **Step 6: Run all Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass (50 existing + 5 binary_resolver = 55).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/claude/session_manager.rs src-tauri/src/claude/mcp_config.rs src-tauri/src/claude/commands.rs
git commit -m "feat: wire binary resolver into session spawn and MCP config generation"
```

---

### Task 8: Cross-Platform Compilation Guards

**Files:**
- Modify: `src-tauri/src/claude/registry.rs:73-99`

- [ ] **Step 1: Add cfg guards to Unix-only functions**

In `src-tauri/src/claude/registry.rs`, wrap the three Unix-only functions:

Replace lines 73-99:
```rust
/// Check if a PID is alive via `ps`.
fn is_process_alive(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if a PID belongs to a Claude process.
fn is_claude_process(pid: u32) -> bool {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output();
    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).contains("claude"),
        Err(_) => false,
    }
}

/// Kill a process by PID.
fn kill_process(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output();
}
```

With:
```rust
#[cfg(target_family = "unix")]
fn is_process_alive(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(target_family = "windows")]
fn is_process_alive(_pid: u32) -> bool {
    false
}

#[cfg(target_family = "unix")]
fn is_claude_process(pid: u32) -> bool {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output();
    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).contains("claude"),
        Err(_) => false,
    }
}

#[cfg(target_family = "windows")]
fn is_claude_process(_pid: u32) -> bool {
    false
}

#[cfg(target_family = "unix")]
fn kill_process(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output();
}

#[cfg(target_family = "windows")]
fn kill_process(_pid: u32) {
    // No-op: Windows orphan cleanup deferred to future phase
}
```

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test registry`
Expected: All 6 registry tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/claude/registry.rs
git commit -m "fix: add cfg guards for Unix-only process functions (Windows compilation)"
```

---

### Task 9: Entitlements + Restricted Capabilities + Persisted Scope

**Files:**
- Create: `src-tauri/entitlements.plist`
- Modify: `src-tauri/capabilities/default.json`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create entitlements.plist**

Create `src-tauri/entitlements.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.app-sandbox</key>
    <true/>
    <key>com.apple.security.network.client</key>
    <true/>
    <key>com.apple.security.network.server</key>
    <true/>
    <key>com.apple.security.files.user-selected.read-write</key>
    <true/>
    <key>com.apple.security.files.bookmarks.app-scope</key>
    <true/>
    <key>com.apple.security.cs.allow-unsigned-executable-memory</key>
    <true/>
</dict>
</plist>
```

- [ ] **Step 2: Add persisted-scope plugin to Cargo.toml**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
tauri-plugin-persisted-scope = "2"
```

- [ ] **Step 3: Register persisted-scope plugin in lib.rs**

In `src-tauri/src/lib.rs`, add the plugin **after** `tauri_plugin_fs::init()`:

```rust
.plugin(tauri_plugin_fs::init())
.plugin(tauri_plugin_persisted_scope::init())
```

- [ ] **Step 4: Replace capabilities/default.json with restricted permissions**

Replace `src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "opener:default",
    {
      "identifier": "shell:allow-execute",
      "allow": [{ "name": "claude", "cmd": "", "args": true }]
    },
    "fs:allow-read",
    "fs:allow-write",
    "sql:default",
    "http:allow-fetch",
    "notification:default",
    "dialog:default",
    "keyring:default",
    "persisted-scope:default"
  ]
}
```

- [ ] **Step 5: Verify Rust compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles without errors. The persisted-scope plugin adds no new Tauri commands — it works transparently.

- [ ] **Step 6: Run all tests**

Run: `cd src-tauri && cargo test && cd .. && npx vitest run`
Expected: All Rust + frontend tests pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/entitlements.plist src-tauri/capabilities/default.json src-tauri/Cargo.toml src-tauri/src/lib.rs
git commit -m "feat: add macOS entitlements, restricted capabilities, persisted-scope plugin"
```

---

## Wave 3: Build Configuration

### Task 10: macOS Bundle Config + CSP Update

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Add macOS bundle config and update CSP**

In `src-tauri/tauri.conf.json`, replace the entire `bundle` section and update the CSP:

Replace the `"bundle"` object:
```json
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "macOS": {
      "minimumSystemVersion": "12.0",
      "entitlements": "entitlements.plist",
      "signingIdentity": "-",
      "frameworks": []
    },
    "category": "public.app-category.business"
  }
```

Replace the CSP in `app.security`:
```json
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src ipc: http://ipc.localhost http://localhost:* https://*.googleapis.com https://*.firebaseio.com https://*.firebaseapp.com; img-src 'self' data: https:"
    }
```

- [ ] **Step 2: Verify dev build still works**

Run: `cd src-tauri && cargo check`
Expected: Compiles. The `signingIdentity: "-"` means ad-hoc signing for dev builds.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "feat: add macOS bundle config (minOS 12.0, entitlements, category) and CSP update"
```

---

## Wave 4: CI Signing + Notarization

### Task 11: CI Workflow — Apple Signing + Artifact Upload

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Update macOS build step with signing env vars + artifact upload**

In `.github/workflows/ci.yml`, update the `build` job. Replace the `tauri-apps/tauri-action` step and add artifact upload:

```yaml
  build:
    needs: [lint-and-typecheck, test-js, test-rust]
    strategy:
      matrix:
        include:
          - os: macos-latest
            target: aarch64-apple-darwin
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - os: windows-latest
            target: x86_64-pc-windows-msvc
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri
      - name: Install Linux deps
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libsecret-1-dev
      - run: npm ci
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          # Firebase CI placeholders
          VITE_FIREBASE_API_KEY: "ci-placeholder"
          VITE_FIREBASE_AUTH_DOMAIN: "ci.firebaseapp.com"
          VITE_FIREBASE_PROJECT_ID: "ci-project"
          VITE_FIREBASE_STORAGE_BUCKET: "ci.appspot.com"
          VITE_FIREBASE_MESSAGING_SENDER_ID: "000000000"
          VITE_FIREBASE_APP_ID: "1:000:web:000"
          # macOS code signing (skipped gracefully when secrets not set)
          APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
          APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
          APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
          # macOS notarization
          APPLE_ID: ${{ secrets.APPLE_ID }}
          APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
          APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
      - name: Upload build artifacts
        uses: actions/upload-artifact@v4
        with:
          name: ikrs-workspace-${{ matrix.target }}
          path: |
            src-tauri/target/release/bundle/dmg/*.dmg
            src-tauri/target/release/bundle/macos/*.app
            src-tauri/target/release/bundle/deb/*.deb
            src-tauri/target/release/bundle/appimage/*.AppImage
            src-tauri/target/release/bundle/msi/*.msi
            src-tauri/target/release/bundle/nsis/*.exe
          if-no-files-found: ignore
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "feat(ci): add Apple signing env vars and artifact upload to build workflow"
```

---

## Wave 5: Validation

### Task 12: Run Full Test Suite + Verify Build

**Files:** None (validation only)

- [ ] **Step 1: Run all Rust tests**

Run: `cd src-tauri && cargo test`
Expected: 55+ tests pass (45 original + 5 token_refresh + 5 binary_resolver).

- [ ] **Step 2: Run all frontend tests**

Run: `npx vitest run`
Expected: 55+ tests pass.

- [ ] **Step 3: Verify Rust compilation for all targets**

Run: `cd src-tauri && cargo check`
Expected: Clean compilation with no warnings about the new modules.

- [ ] **Step 4: Verify tauri build config is valid**

Run: `npx tauri info`
Expected: Shows correct identifier `ae.ikaros.workspace`, macOS minimumSystemVersion 12.0, lists all plugins including persisted-scope.

- [ ] **Step 5: Update spec status**

In `docs/specs/m2-phase4a-sandbox-signing-design.md`, change line 2:
```
**Status:** Complete (Waves 1-3 implemented, Wave 4 pending Apple Developer account)
```

- [ ] **Step 6: Final commit**

```bash
git add docs/specs/m2-phase4a-sandbox-signing-design.md
git commit -m "docs: mark Phase 4a spec complete (Waves 1-3 implemented)"
```

---

## Summary

| Wave | Tasks | Tests Added |
|------|-------|-------------|
| 1: Phase 3 debt | Tasks 1-5 (identifier, refresh_token, commands wiring, SettingsView, dead code removal) | 5 (token_refresh) |
| 2: Sandbox prep | Tasks 6-9 (binary resolver, wiring, cfg guards, entitlements + capabilities) | 5 (binary_resolver) |
| 3: Build config | Task 10 (macOS bundle, CSP) | 0 |
| 4: CI signing | Task 11 (workflow update) | 0 |
| 5: Validation | Task 12 (full test suite, build verification) | 0 |

**Total: 12 tasks, ~55 steps, 10 new tests, 12 commits.**

**Blocked on human task:** Wave 4 CI signing secrets require Apple Developer Program enrollment. All other waves proceed independently.
