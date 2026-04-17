# M2 Phase 3b Implementation Plan: MCP Wiring + Token Resilience

**Spec:** `docs/specs/m2-phase3b-mcp-wiring-design.md`
**Date:** 2026-04-12
**Status:** Complete — all 10 tasks implemented, 12 commits on main
**Tasks:** 10 tasks in 4 waves
**Codex checkpoints:** Checkpoint 1 PASS 8/10 (I1 fixed, I2 deferred-low), Checkpoint 2 pending

---

## Wave 1: Rust Backend — Config Generation + Spawn Changes (sequential)

### Task 1: Create `mcp_config.rs` — MCP config generator

**File:** `src-tauri/src/claude/mcp_config.rs` (CREATE)

Create a new Rust module that generates `.mcp-config.json` per engagement.

```rust
// src-tauri/src/claude/mcp_config.rs

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
        servers.insert("gmail".to_string(), McpServerEntry {
            command: "npx".to_string(),
            args: vec!["@shinzolabs/gmail-mcp@1.7.4".to_string()],
            env: HashMap::from([
                ("GOOGLE_ACCESS_TOKEN".to_string(), "${GOOGLE_ACCESS_TOKEN}".to_string()),
            ]),
        });
        servers.insert("calendar".to_string(), McpServerEntry {
            command: "npx".to_string(),
            args: vec!["@cocal/google-calendar-mcp@2.6.1".to_string()],
            env: HashMap::from([
                ("GOOGLE_ACCESS_TOKEN".to_string(), "${GOOGLE_ACCESS_TOKEN}".to_string()),
            ]),
        });
        servers.insert("drive".to_string(), McpServerEntry {
            command: "npx".to_string(),
            args: vec!["@piotr-agier/google-drive-mcp@2.0.2".to_string()],
            env: HashMap::from([
                ("GOOGLE_ACCESS_TOKEN".to_string(), "${GOOGLE_ACCESS_TOKEN}".to_string()),
            ]),
        });
    }

    if let Some(vp) = vault_path {
        if vp.exists() {
            servers.insert("obsidian".to_string(), McpServerEntry {
                command: "npx".to_string(),
                args: vec![
                    "@bitbonsai/mcpvault@1.3.0".to_string(),
                    vp.to_string_lossy().to_string(),
                ],
                env: HashMap::new(),
            });
        }
    }

    let config = McpConfig { mcp_servers: servers };
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize MCP config: {e}"))?;

    let config_path = engagement_path.join(".mcp-config.json");
    // Atomic write: tmp + rename
    let tmp_path = engagement_path.join(".mcp-config.json.tmp");
    std::fs::write(&tmp_path, &json)
        .map_err(|e| format!("Failed to write MCP config: {e}"))?;
    std::fs::rename(&tmp_path, &config_path)
        .map_err(|e| format!("Failed to rename MCP config: {e}"))?;

    Ok(config_path)
}
```

**Also modify:** `src-tauri/src/claude/mod.rs` — add `pub mod mcp_config;`

**Tests (in module):**
1. `test_generate_config_with_google_token` — generates config with 3 Google servers + obsidian
2. `test_generate_config_no_token` — no Google servers, only obsidian if vault exists
3. `test_generate_config_no_vault` — Google servers only, no obsidian
4. `test_generate_config_empty` — no token, no vault = empty mcpServers

**Commit message:** `feat(mcp): add MCP config generator for per-engagement .mcp-config.json`

---

### Task 2: Extend `session_manager.rs` spawn() with env vars and --mcp-config

**File:** `src-tauri/src/claude/session_manager.rs` (MODIFY)

Changes to `spawn()` signature — add two parameters:

```rust
pub async fn spawn(
    &self,
    engagement_id: String,
    engagement_path: String,
    resume_session_id: Option<String>,
    env_vars: HashMap<String, String>,        // NEW
    mcp_config_path: Option<String>,          // NEW
    app: AppHandle,
) -> Result<(String, u32), String>
```

Changes to spawn body:
1. After building `args` vec (line 61-70), add:
   ```rust
   if let Some(ref config_path) = mcp_config_path {
       args.push("--mcp-config".to_string());
       args.push(config_path.clone());
   }
   ```

2. After `Command::new("claude")` (line 77), add env injection:
   ```rust
   let mut child = Command::new("claude")
       .args(&args)
       .current_dir(&engagement_path)
       .envs(&env_vars)                       // NEW
       .stdin(std::process::Stdio::piped())
       .stdout(std::process::Stdio::piped())
       .stderr(std::process::Stdio::piped())
       .spawn()
       .map_err(|e| format!("Failed to spawn claude: {e}"))?;
   ```

**Update existing test** `test_session_removed_after_kill` — no signature changes needed (it doesn't call spawn).

**Commit message:** `feat(mcp): extend spawn() with env_vars and --mcp-config flag`

---

### Task 3: Update `commands.rs` — orchestrate MCP config generation

**File:** `src-tauri/src/claude/commands.rs` (MODIFY)

Rewrite `spawn_claude_session` to add `client_slug` param and orchestrate config generation.

**Keychain pattern:** Uses `KeyringExt` trait (same as `credentials.rs`). Service name: `"ikrs-workspace"`. Key format: `ikrs:{engagement_id}:google` (via `make_keychain_key`).

**`client_slug` is `Option<String>`** — engagements without linked clients skip vault creation and MCP config (Codex I3 fix).

```rust
use tauri_plugin_keyring::KeyringExt;
use crate::commands::credentials::make_keychain_key;

const IKRS_SERVICE: &str = "ikrs-workspace";

#[tauri::command]
pub async fn spawn_claude_session(
    engagement_id: String,
    engagement_path: String,
    resume_session_id: Option<String>,
    client_slug: Option<String>,            // NEW — Option because engagements may lack client
    state: State<'_, ClaudeSessionManager>,
    app: AppHandle,
) -> Result<String, String> {
    let mut env_vars = std::collections::HashMap::new();
    let mut mcp_config_path: Option<String> = None;

    // 1. Read Google OAuth token from keychain (KeyringExt pattern)
    let keychain_key = make_keychain_key(&engagement_id, "google");
    let google_token = app.keyring()
        .get_password(IKRS_SERVICE, &keychain_key)
        .ok()
        .flatten();
    let has_token = google_token.is_some();
    if let Some(ref token) = google_token {
        env_vars.insert("GOOGLE_ACCESS_TOKEN".to_string(), token.clone());
    }

    // 2. Resolve vault path and ensure directory exists (Codex C1)
    //    Only if client_slug is provided (engagements without clients skip MCP)
    if let Some(ref slug) = client_slug {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let vault_path = std::path::PathBuf::from(&home)
            .join(".ikrs-workspace")
            .join("vaults")
            .join(slug);
        if !vault_path.exists() {
            let _ = std::fs::create_dir_all(&vault_path);
        }

        // 3. Generate MCP config
        let engagement_dir = std::path::Path::new(&engagement_path);
        let config_path = crate::claude::mcp_config::generate_mcp_config(
            engagement_dir,
            has_token,
            Some(&vault_path),
        )?;
        mcp_config_path = Some(config_path.to_string_lossy().to_string());
    }

    // 4. Spawn Claude with MCP config
    let (session_id, child_pid) = state
        .spawn(
            engagement_id.clone(),
            engagement_path,
            resume_session_id,
            env_vars,
            mcp_config_path,
            app.clone(),
        )
        .await?;

    // 5. Register in session registry
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("No app data dir: {e}"))?;
    let _ = crate::claude::registry::register_session(
        &app_data_dir,
        &engagement_id,
        &session_id,
        child_pid,
    );

    Ok(session_id)
}
```

**Commit message:** `feat(mcp): orchestrate MCP config + env vars in spawn command`

---

## Wave 2: Retirement + Frontend Wiring (parallelizable)

### Task 4: Delete McpProcessManager (Rust side)

**Files:**
- `src-tauri/src/mcp/manager.rs` — DELETE
- `src-tauri/src/mcp/mod.rs` — DELETE
- `src-tauri/src/commands/mcp.rs` — DELETE
- `src-tauri/src/commands/mod.rs` — MODIFY: remove `pub mod mcp;` line
- `src-tauri/src/lib.rs` — MODIFY:
  - Remove `mod mcp;` (line 3)
  - Remove `use mcp::manager::McpProcessManager;` (line 8)
  - Remove `.manage(McpProcessManager::new())` (line 23)
  - Remove 5 MCP commands from invoke_handler (lines 36-40):
    ```
    commands::mcp::spawn_mcp,
    commands::mcp::kill_mcp,
    commands::mcp::kill_all_mcp,
    commands::mcp::mcp_health,
    commands::mcp::restart_mcp,
    ```

**Verification:** `cargo check` must pass after deletion.

**Commit message:** `refactor(mcp): retire McpProcessManager — Claude CLI owns MCP lifecycle`

---

### Task 5: Delete `useEngagement.ts` + clean frontend MCP commands

**Files:**
- `src/hooks/useEngagement.ts` — DELETE entirely
- `src/lib/tauri-commands.ts` — MODIFY:
  - Delete `SpawnMcpArgs` interface (lines 57-63)
  - Delete `McpStatusResult` interface (lines 50-55)
  - Delete `spawnMcp` function (lines 64-66)
  - Delete `killMcp` function (lines 68-70)
  - Delete `killAllMcp` function (lines 72-74)
  - Delete `mcpHealth` function (lines 76-80)
  - Delete `restartMcp` function (lines 82-86)
  - Delete `McpServerType` type export (line 48) — this type stays in `src/types/index.ts`
  - Update `spawnClaudeSession` to add `clientSlug` param:
    ```typescript
    export async function spawnClaudeSession(
      engagementId: string,
      engagementPath: string,
      resumeSessionId?: string,
      clientSlug?: string,
    ): Promise<string> {
      return invoke("spawn_claude_session", {
        engagementId,
        engagementPath,
        resumeSessionId: resumeSessionId ?? null,
        clientSlug: clientSlug ?? null,
      });
    }
    ```

**Verification:**
1. Check no other file imports from `useEngagement.ts`. Grep for `useEngagement`, `spawnMcp`, `killMcp`, `killAllMcp`, `mcpHealth`, `restartMcp` — all should be zero results after cleanup.
2. **mcpStore consumer audit (Codex I1):** Verify that `App.tsx`, `useDrive.ts`, `useNotes.ts`, `useGmail.ts`, `useCalendar.ts` gracefully handle empty `mcpStore.servers`. After this phase, `setServers()` is never called — these consumers will always see "no servers" until Phase 3c+ wires `system.init` parsing. Confirm each consumer handles the empty/null case without crashing.

**Commit message:** `refactor(mcp): remove frontend MCP commands + useEngagement hook`

---

### Task 6: Update `useWorkspaceSession.ts` to pass clientSlug

**File:** `src/hooks/useWorkspaceSession.ts` (MODIFY)

Update both `connect()` and `switchEngagement()` to resolve client slug and pass it to `spawnClaudeSession`. There are **4 call sites** that need updating:

**Call site 1 — `connect()` primary spawn (line 68):**
```typescript
const client = useEngagementStore.getState().clients.find(
  (c) => c.id === engagement.clientId
);
await spawnClaudeSession(
  engagement.id,
  engagement.vault.path,
  resumeId ?? undefined,
  client?.slug,
);
```

**Call site 2 — `connect()` fallback spawn after resume timeout (line 81):**
```typescript
await spawnClaudeSession(engagement.id, engagement.vault.path, undefined, client?.slug);
```
Note: `client` is already resolved above, reuse it.

**Call site 3 — `switchEngagement()` primary spawn (lines 119-122):**
```typescript
const engagement = useEngagementStore.getState().engagements.find(
  (e) => e.id === newEngagementId
);
const switchClient = useEngagementStore.getState().clients.find(
  (c) => c.id === engagement?.clientId
);
if (engagement) {
  useClaudeStore.setState({ status: "connecting" });
  await spawnClaudeSession(
    newEngagementId,
    engagement.vault.path,
    resumeId ?? undefined,
    switchClient?.slug,
  );
```

**Call site 4 — `switchEngagement()` fallback spawn (line 133):**
```typescript
await spawnClaudeSession(newEngagementId, engagement.vault.path, undefined, switchClient?.slug);
```

**Commit message:** `feat(mcp): pass clientSlug through workspace session to spawn`

---

## Codex Checkpoint 1 — After Wave 2 (Tasks 1-6)

Verify: `cargo check`, config generation works, MCP module fully removed, frontend compiles.

---

## Wave 3: Token Resilience (sequential)

### Task 7: Add `McpAuthErrorPayload` type to Rust + TypeScript

**Files:**
- `src-tauri/src/claude/types.rs` — MODIFY: add:
  ```rust
  #[derive(Debug, Clone, Serialize)]
  pub struct McpAuthErrorPayload {
      pub server_name: String,
      pub error_hint: String,
  }
  ```

- `src/types/claude.ts` — MODIFY: add:
  ```typescript
  export interface McpAuthErrorPayload {
    server_name: string;
    error_hint: string;
  }
  ```

**Commit message:** `feat(mcp): add McpAuthErrorPayload type for auth-error detection`

---

### Task 8: Add auth-error detection to `stream_parser.rs`

**File:** `src-tauri/src/claude/stream_parser.rs` (MODIFY)

Add auth-error pattern detection in `handle_user_event()` where `tool_result` blocks are processed. When a tool result has `is_error: true` and the content matches auth keywords, emit `claude:mcp-auth-error`.

Add after the existing `tool_result` handling (around line 268):

```rust
// Auth-error detection for MCP tools
if is_error {
    if let Some(content_val) = &content_ref {
        let content_str = match content_val {
            serde_json::Value::String(s) => s.clone(),
            other => serde_json::to_string(other).unwrap_or_default(),
        };
        if is_auth_error(&content_str) {
            let server = infer_mcp_server(tool_id);
            let _ = app.emit(
                "claude:mcp-auth-error",
                McpAuthErrorPayload {
                    server_name: server,
                    error_hint: cap_string(&content_str, 200),
                },
            );
        }
    }
}
```

Helper functions:
```rust
fn is_auth_error(content: &str) -> bool {
    let lower = content.to_lowercase();
    lower.contains("401")
        || lower.contains("403")
        || lower.contains("token expired")
        || lower.contains("authentication failed")
        || lower.contains("invalid_grant")
        || lower.contains("unauthenticated")
}

/// Infer which MCP server a tool belongs to from the tool_id prefix.
/// MCP tools are typically prefixed: mcp__gmail__*, mcp__calendar__*, etc.
fn infer_mcp_server(tool_id: &str) -> String {
    if tool_id.contains("gmail") { return "gmail".to_string(); }
    if tool_id.contains("calendar") { return "calendar".to_string(); }
    if tool_id.contains("drive") { return "drive".to_string(); }
    if tool_id.contains("obsidian") { return "obsidian".to_string(); }
    "unknown".to_string()
}
```

**Tests:**
1. `test_is_auth_error_401` — "HTTP 401 Unauthorized" → true
2. `test_is_auth_error_token_expired` — "The token expired at..." → true
3. `test_is_auth_error_normal` — "File not found" → false
4. `test_infer_mcp_server_gmail` — "mcp__gmail__read" → "gmail"
5. `test_infer_mcp_server_unknown` — "toolu_abc123" → "unknown"

**Commit message:** `feat(mcp): detect auth errors in MCP tool results and emit event`

---

### Task 9: Frontend auth-error listener + store state

**Files:**
- `src/stores/claudeStore.ts` — MODIFY: add `authError` state:
  ```typescript
  authError: { server: string; hint: string } | null;
  setAuthError: (server: string, hint: string) => void;
  clearAuthError: () => void;
  ```
  Add to `initialState`: `authError: null`
  Add to reset: clear authError

- `src/hooks/useClaudeStream.ts` — MODIFY: add listener:
  ```typescript
  unlisteners.push(
    await listen<McpAuthErrorPayload>("claude:mcp-auth-error", (event) => {
      store().setAuthError(
        event.payload.server_name,
        event.payload.error_hint
      );
    })
  );
  ```
  Add `McpAuthErrorPayload` to imports from `@/types/claude`.

**Commit message:** `feat(mcp): add auth-error state and stream listener`

---

### Task 10: Auth-error toast + re-auth flow in ChatView

**File:** `src/views/ChatView.tsx` (MODIFY)

Add auth-error toast/banner with **full re-auth wiring** below the error block (after line 104). The spec requires both detection AND re-auth flow — the button must trigger OAuth, store the fresh token, kill the session, and respawn (Codex C1 fix).

```tsx
const authError = useClaudeStore((s) => s.authError);
const clearAuthError = useClaudeStore((s) => s.clearAuthError);
const [reauthing, setReauthing] = useState(false);

// Handler for re-auth button
const handleReauth = useCallback(async () => {
  if (reauthing) return;
  setReauthing(true);
  clearAuthError();
  try {
    // 1. Trigger Google OAuth (uses existing commands)
    const { auth_url } = await startOAuth(
      GOOGLE_CLIENT_ID,  // from app config/env
      8765,              // redirect port (matches existing OAuth setup)
      ["https://www.googleapis.com/auth/gmail.modify",
       "https://www.googleapis.com/auth/calendar",
       "https://www.googleapis.com/auth/drive"]
    );
    // 2. Open browser for auth (user completes OAuth)
    await open(auth_url);
    // Note: exchangeOAuthCode is called by the OAuth redirect handler
    // which stores the token via storeCredential. Once stored:
    
    // 3. Kill current session
    const sid = useClaudeStore.getState().sessionId;
    if (sid) await killClaudeSession(sid);
    
    // 4. Reconnect (spawns with fresh token from keychain)
    await handleConnect();
  } catch (e) {
    useClaudeStore.getState().setError(
      `Re-auth failed: ${e instanceof Error ? e.message : String(e)}`
    );
  } finally {
    setReauthing(false);
  }
}, [reauthing, clearAuthError, handleConnect]);

// In JSX, after the error block:
{authError && (
  <div className="flex items-center gap-2 p-3 rounded-md bg-amber-500/10 text-amber-700 dark:text-amber-400 text-sm">
    <span>
      Google authentication expired for {authError.server}. Re-authenticate to restore access.
    </span>
    <Button
      variant="outline"
      size="sm"
      onClick={handleReauth}
      disabled={reauthing}
      className="ml-auto"
    >
      {reauthing ? "Re-authenticating..." : "Re-authenticate"}
    </Button>
  </div>
)}
```

**Imports to add:** `startOAuth`, `killClaudeSession` from `@/lib/tauri-commands`; `open` from `@tauri-apps/plugin-opener`; `useState` (already imported).

**Note:** The OAuth redirect handler (already exists in the app) calls `exchangeOAuthCode` and `storeCredential` when the user completes the browser OAuth flow. Task 10 triggers the flow and then reconnects after. The `GOOGLE_CLIENT_ID` and redirect port should match the existing OAuth config used elsewhere in the app.

**Commit message:** `feat(mcp): auth-error toast with full re-auth flow in ChatView`

---

## Wave 4: Spec Alignment + Documentation

### (No code task) Amend parent spec

**File:** `docs/specs/embedded-claude-architecture.md` — MODIFY

Find the Phase 3 Q3 resolution text that says "generated at scaffold time" and change to "generated at session spawn time." This is a one-line change.

**Commit message:** `docs: amend Phase 3 Q3 — MCP config generated at spawn time, not scaffold time`

---

## Codex Checkpoint 2 — Final Review (all 10 tasks)

Full review: spec compliance, code quality, zero orphan references, cargo check, TypeScript compile.

---

## Summary

| Wave | Tasks | Parallel? | Description |
|------|-------|-----------|-------------|
| 1 | 1, 2, 3 | Sequential | Rust: config gen → spawn changes → command orchestration |
| 2 | 4, 5, 6 | Parallel (4∥5, then 6) | Retirement: delete Rust MCP + delete frontend MCP + wire clientSlug |
| 3 | 7, 8, 9, 10 | Sequential | Token resilience: types → parser → store → UI |
| 4 | doc | Single | Spec amendment |

**Total commits:** 10 feature + fixes from Codex reviews
**Codex checkpoints:** 2 (mid + final)
