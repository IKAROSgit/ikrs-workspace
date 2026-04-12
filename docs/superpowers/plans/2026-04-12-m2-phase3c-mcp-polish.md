# M2 Phase 3c: MCP Polish + Test Coverage — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close Phase 3 by wiring MCP server health from session-ready, fixing re-auth timing with a redirect capture server, adding strict MCP mode, fixing auth-error server inference, and adding frontend test coverage.

**Architecture:** Rust backend adds a one-shot TCP redirect server for OAuth callback capture, a tool_name mapping in the stream parser, and a strict_mcp guard. TypeScript frontend adds an extractMcpServers utility, two-step re-auth flow, and Vitest tests with Tauri API mocks.

**Tech Stack:** Rust (Tauri 2, tokio), TypeScript (React 19, Zustand v5), Vitest, TDD

**Spec:** `docs/specs/m2-phase3c-mcp-polish-design.md` (Codex PASS 9/10)

---

## File Map

| File | Action | Task |
|------|--------|------|
| `src/lib/mcp-utils.ts` | CREATE | 1 |
| `tests/setup.ts` | CREATE | 2 |
| `tests/unit/lib/mcp-utils.test.ts` | CREATE | 1 |
| `tests/unit/stores/mcpStore.test.ts` | CREATE | 3 |
| `tests/unit/stores/claudeStore-auth.test.ts` | CREATE | 4 |
| `src/hooks/useClaudeStream.ts` | MODIFY | 5 |
| `src-tauri/src/claude/stream_parser.rs` | MODIFY | 6 |
| `src-tauri/src/oauth/redirect_server.rs` | CREATE | 7 |
| `src-tauri/src/oauth/mod.rs` | MODIFY | 7 |
| `src-tauri/src/commands/oauth.rs` | MODIFY | 8 |
| `src-tauri/src/lib.rs` | MODIFY | 8 |
| `src/lib/tauri-commands.ts` | MODIFY | 9 |
| `src/views/ChatView.tsx` | MODIFY | 9 |
| `src/types/index.ts` | MODIFY | 10 |
| `src-tauri/src/claude/commands.rs` | MODIFY | 10 |
| `src/hooks/useWorkspaceSession.ts` | MODIFY | 10 |

---

## Wave 1: MCP Health Wiring + Tests (Tasks 1-5)

### Task 1: Create `extractMcpServers` utility with tests (TDD)

**Files:**
- Create: `src/lib/mcp-utils.ts`
- Create: `tests/setup.ts`
- Create: `tests/unit/lib/mcp-utils.test.ts`

- [ ] **Step 1: Create test setup file with Tauri API mocks**

Create `tests/setup.ts`:

```typescript
import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  open: vi.fn(),
}));
```

- [ ] **Step 2: Write failing tests for extractMcpServers**

Create `tests/unit/lib/mcp-utils.test.ts`:

```typescript
import { describe, it, expect } from "vitest";
import { extractMcpServers } from "@/lib/mcp-utils";

describe("extractMcpServers", () => {
  it("extracts gmail, calendar, drive, obsidian from mixed tool list", () => {
    const tools = [
      "Read", "Write", "Edit", "Glob", "Grep",
      "mcp__gmail__read_message", "mcp__gmail__search_messages",
      "mcp__calendar__list_events", "mcp__drive__list_files",
      "mcp__obsidian__read_note",
    ];
    const result = extractMcpServers(tools);
    const types = result.map((s) => s.type).sort();
    expect(types).toEqual(["calendar", "drive", "gmail", "obsidian"]);
  });

  it("ignores non-MCP tools", () => {
    const tools = ["Read", "Write", "Edit", "Glob", "Grep", "WebSearch"];
    const result = extractMcpServers(tools);
    expect(result).toEqual([]);
  });

  it("deduplicates multiple tools from same server", () => {
    const tools = [
      "mcp__gmail__read_message",
      "mcp__gmail__search_messages",
      "mcp__gmail__send_message",
    ];
    const result = extractMcpServers(tools);
    expect(result).toHaveLength(1);
    expect(result[0].type).toBe("gmail");
  });

  it("returns empty array for no MCP tools", () => {
    expect(extractMcpServers([])).toEqual([]);
  });

  it("ignores unknown MCP prefixes", () => {
    const tools = ["mcp__slack__send", "mcp__notion__read"];
    expect(extractMcpServers(tools)).toEqual([]);
  });

  it("populates restartCount: 0 and status: healthy", () => {
    const tools = ["mcp__gmail__read_message"];
    const result = extractMcpServers(tools);
    expect(result[0].status).toBe("healthy");
    expect(result[0].restartCount).toBe(0);
    expect(result[0].lastPing).toBeInstanceOf(Date);
  });
});
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run tests/unit/lib/mcp-utils.test.ts`
Expected: FAIL — module `@/lib/mcp-utils` not found

- [ ] **Step 4: Implement extractMcpServers**

Create `src/lib/mcp-utils.ts`:

```typescript
import type { McpHealth, McpServerType } from "@/types";

const MCP_PREFIX_MAP: Record<string, McpServerType> = {
  gmail: "gmail",
  calendar: "calendar",
  drive: "drive",
  obsidian: "obsidian",
};

export function extractMcpServers(tools: string[]): McpHealth[] {
  const found = new Set<McpServerType>();
  for (const tool of tools) {
    const match = tool.match(/^mcp__(\w+)__/);
    if (match) {
      const serverType = MCP_PREFIX_MAP[match[1]];
      if (serverType) found.add(serverType);
    }
  }
  return Array.from(found).map((type) => ({
    type,
    status: "healthy" as const,
    lastPing: new Date(),
    restartCount: 0,
  }));
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run tests/unit/lib/mcp-utils.test.ts`
Expected: 6 tests PASS

- [ ] **Step 6: Commit**

```bash
git add tests/setup.ts tests/unit/lib/mcp-utils.test.ts src/lib/mcp-utils.ts
git commit -m "feat(mcp): add extractMcpServers utility with tests"
```

---

### Task 2: mcpStore tests

**Files:**
- Create: `tests/unit/stores/mcpStore.test.ts`

- [ ] **Step 1: Write mcpStore tests**

Create `tests/unit/stores/mcpStore.test.ts`:

```typescript
import { describe, it, expect, beforeEach } from "vitest";
import { useMcpStore } from "@/stores/mcpStore";
import type { McpHealth } from "@/types";

describe("mcpStore", () => {
  beforeEach(() => {
    useMcpStore.setState({ servers: [] });
  });

  it("setServers populates server list", () => {
    const servers: McpHealth[] = [
      { type: "gmail", status: "healthy", lastPing: new Date(), restartCount: 0 },
      { type: "drive", status: "healthy", lastPing: new Date(), restartCount: 0 },
    ];
    useMcpStore.getState().setServers(servers);
    expect(useMcpStore.getState().servers).toHaveLength(2);
    expect(useMcpStore.getState().servers[0].type).toBe("gmail");
  });

  it("setServerHealth updates individual server status", () => {
    useMcpStore.getState().setServers([
      { type: "gmail", status: "healthy", lastPing: new Date(), restartCount: 0 },
    ]);
    useMcpStore.getState().setServerHealth("gmail", "down");
    expect(useMcpStore.getState().servers[0].status).toBe("down");
  });

  it("setServers with empty array clears all servers", () => {
    useMcpStore.getState().setServers([
      { type: "gmail", status: "healthy", lastPing: new Date(), restartCount: 0 },
    ]);
    useMcpStore.getState().setServers([]);
    expect(useMcpStore.getState().servers).toEqual([]);
  });

  it("consumer can find server by type", () => {
    useMcpStore.getState().setServers([
      { type: "gmail", status: "healthy", lastPing: new Date(), restartCount: 0 },
      { type: "drive", status: "down", lastPing: new Date(), restartCount: 0 },
    ]);
    const gmail = useMcpStore.getState().servers.find((s) => s.type === "gmail");
    expect(gmail?.status).toBe("healthy");
    const drive = useMcpStore.getState().servers.find((s) => s.type === "drive");
    expect(drive?.status).toBe("down");
  });
});
```

- [ ] **Step 2: Run tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run tests/unit/stores/mcpStore.test.ts`
Expected: 4 tests PASS

- [ ] **Step 3: Commit**

```bash
git add tests/unit/stores/mcpStore.test.ts
git commit -m "test(mcp): add mcpStore unit tests"
```

---

### Task 3: claudeStore auth-error tests

**Files:**
- Create: `tests/unit/stores/claudeStore-auth.test.ts`

- [ ] **Step 1: Write claudeStore auth-error tests**

Create `tests/unit/stores/claudeStore-auth.test.ts`:

```typescript
import { describe, it, expect, beforeEach } from "vitest";
import { useClaudeStore } from "@/stores/claudeStore";

describe("claudeStore auth-error state", () => {
  beforeEach(() => {
    useClaudeStore.getState().reset();
  });

  it("setAuthError stores server and hint", () => {
    useClaudeStore.getState().setAuthError("gmail", "Token expired");
    const { authError } = useClaudeStore.getState();
    expect(authError).toEqual({ server: "gmail", hint: "Token expired" });
  });

  it("clearAuthError resets to null", () => {
    useClaudeStore.getState().setAuthError("gmail", "Token expired");
    useClaudeStore.getState().clearAuthError();
    expect(useClaudeStore.getState().authError).toBeNull();
  });

  it("reset() clears authError", () => {
    useClaudeStore.getState().setAuthError("drive", "HTTP 401");
    useClaudeStore.getState().reset();
    expect(useClaudeStore.getState().authError).toBeNull();
  });

  it("setDisconnected clears session but preserves authError", () => {
    useClaudeStore.setState({ sessionId: "sess_1", status: "connected" });
    useClaudeStore.getState().setAuthError("gmail", "expired");
    useClaudeStore.getState().setDisconnected("process exited");
    expect(useClaudeStore.getState().sessionId).toBeNull();
    expect(useClaudeStore.getState().status).toBe("disconnected");
    expect(useClaudeStore.getState().authError).toEqual({ server: "gmail", hint: "expired" });
  });
});
```

- [ ] **Step 2: Run tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run tests/unit/stores/claudeStore-auth.test.ts`
Expected: 4 tests PASS

- [ ] **Step 3: Commit**

```bash
git add tests/unit/stores/claudeStore-auth.test.ts
git commit -m "test(mcp): add claudeStore auth-error state tests"
```

---

### Task 4: Wire MCP health from session-ready + clear on disconnect

**Files:**
- Modify: `src/hooks/useClaudeStream.ts`

- [ ] **Step 1: Add mcpStore import and wire session-ready to populate MCP servers**

In `src/hooks/useClaudeStream.ts`, add import at top:

```typescript
import { useMcpStore } from "@/stores/mcpStore";
import { extractMcpServers } from "@/lib/mcp-utils";
```

Inside the `claude:session-ready` listener (after `store().setSessionReady(...)`), add:

```typescript
          const mcpServers = extractMcpServers(event.payload.tools);
          useMcpStore.getState().setServers(mcpServers);
```

- [ ] **Step 2: Clear mcpStore on session-ended**

Inside the `claude:session-ended` listener (after `store().setDisconnected(...)`), add:

```typescript
          useMcpStore.getState().setServers([]);
```

- [ ] **Step 3: Run all tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add src/hooks/useClaudeStream.ts
git commit -m "feat(mcp): wire session-ready tools to mcpStore, clear on disconnect"
```

---

## Wave 2: Auth-Error Fix + OAuth Redirect Server (Tasks 5-8)

### Task 5: Fix tool_id→tool_name mapping in stream_parser.rs (Codex C2)

**Files:**
- Modify: `src-tauri/src/claude/stream_parser.rs`

- [ ] **Step 1: Add tool_name_map to parse_stream and handle_line**

In `parse_stream`, after `let mut current_msg_id = msg_id_gen.next();`, add:

```rust
    let mut tool_name_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
```

Update the `handle_line` call to pass `&mut tool_name_map`:

```rust
                handle_line(&line, &app, &mut msg_id_gen, &mut current_msg_id, &mut tool_name_map);
```

Update `handle_line` signature to accept the map:

```rust
fn handle_line(
    line: &str,
    app: &AppHandle,
    msg_id_gen: &mut MessageIdGen,
    current_msg_id: &mut String,
    tool_name_map: &mut std::collections::HashMap<String, String>,
)
```

Pass `tool_name_map` to `handle_assistant_event` and `handle_user_event`:

```rust
        "assistant" => handle_assistant_event(&raw, app, msg_id_gen, current_msg_id, tool_name_map),
        "user" => handle_user_event(&raw, app, tool_name_map),
```

- [ ] **Step 2: Update handle_assistant_event to insert into tool_name_map**

Add `tool_name_map: &mut std::collections::HashMap<String, String>` to the function signature.

After the `tool_use` arm emits `claude:tool-start`, insert the mapping:

```rust
                tool_name_map.insert(tool_id.to_string(), tool_name.to_string());
```

- [ ] **Step 3: Update handle_user_event to lookup tool_name from map**

Add `tool_name_map: &std::collections::HashMap<String, String>` to the function signature.

Replace the `infer_mcp_server(tool_id)` call (in the auth-error detection block) with:

```rust
                        let resolved_name = tool_name_map.get(tool_id).map(|s| s.as_str()).unwrap_or("");
                        let server = infer_mcp_server(resolved_name);
```

- [ ] **Step 4: Update infer_mcp_server to use mcp__ prefix pattern**

Replace the existing `infer_mcp_server` function:

```rust
fn infer_mcp_server(tool_name: &str) -> String {
    if tool_name.starts_with("mcp__gmail__") {
        return "gmail".to_string();
    }
    if tool_name.starts_with("mcp__calendar__") {
        return "calendar".to_string();
    }
    if tool_name.starts_with("mcp__drive__") {
        return "drive".to_string();
    }
    if tool_name.starts_with("mcp__obsidian__") {
        return "obsidian".to_string();
    }
    "unknown".to_string()
}
```

- [ ] **Step 5: Update existing tests for infer_mcp_server**

Replace the two existing tests:

```rust
    #[test]
    fn test_infer_mcp_server_gmail() {
        assert_eq!(infer_mcp_server("mcp__gmail__read_message"), "gmail");
    }

    #[test]
    fn test_infer_mcp_server_unknown() {
        assert_eq!(infer_mcp_server("Read"), "unknown");
    }
```

- [ ] **Step 6: Run Rust tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && export PATH="$HOME/.cargo/bin:$PATH" && cargo test --lib`
Expected: All tests PASS

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/claude/stream_parser.rs
git commit -m "fix(mcp): use tool_name_map for auth-error server inference (Codex C2)"
```

---

### Task 6: Create OAuth redirect capture server

**Files:**
- Create: `src-tauri/src/oauth/redirect_server.rs`
- Modify: `src-tauri/src/oauth/mod.rs`

- [ ] **Step 1: Create redirect_server.rs**

Create `src-tauri/src/oauth/redirect_server.rs`:

```rust
use tauri::{AppHandle, Emitter};
use tauri_plugin_keyring::KeyringExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const IKRS_SERVICE: &str = "ikrs-workspace";

/// Try to bind a TcpListener on localhost, scanning from preferred_port up to +10.
async fn bind_with_fallback(preferred_port: u16) -> Result<(TcpListener, u16), String> {
    for port in preferred_port..=preferred_port + 10 {
        match TcpListener::bind(format!("127.0.0.1:{port}")).await {
            Ok(listener) => return Ok((listener, port)),
            Err(_) => continue,
        }
    }
    Err(format!(
        "Could not bind to any port in range {}-{}",
        preferred_port,
        preferred_port + 10
    ))
}

/// Extract the `code` query parameter from an HTTP GET request line.
fn extract_code(request: &str) -> Option<String> {
    let path = request.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some("code") {
            return parts.next().map(|v| urlencoding::decode(v).unwrap_or_default().to_string());
        }
    }
    None
}

const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html><head><title>IKAROS Workspace</title>
<style>body{font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#f5f5f5}
.card{background:white;padding:2rem;border-radius:12px;box-shadow:0 2px 8px rgba(0,0,0,0.1);text-align:center}
h1{color:#22c55e;font-size:1.5rem}p{color:#666}</style></head>
<body><div class="card"><h1>Sign-in complete</h1><p>You can close this tab and return to IKAROS Workspace.</p></div></body></html>"#;

/// Starts a one-shot HTTP server that captures the OAuth redirect code,
/// exchanges it for tokens, stores the access token in the keychain,
/// and emits `oauth:token-stored`.
///
/// Returns (JoinHandle, actual_port).
pub async fn start_redirect_server(
    preferred_port: u16,
    client_id: String,
    verifier: String,
    keychain_key: String,
    app: AppHandle,
) -> Result<(tokio::task::JoinHandle<Result<(), String>>, u16), String> {
    let (listener, actual_port) = bind_with_fallback(preferred_port).await?;

    let handle = tokio::spawn(async move {
        let (mut stream, _addr) = listener
            .accept()
            .await
            .map_err(|e| format!("Accept failed: {e}"))?;

        let mut buf = vec![0u8; 4096];
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| format!("Read failed: {e}"))?;
        let request = String::from_utf8_lossy(&buf[..n]);

        let code = extract_code(&request)
            .ok_or_else(|| "No authorization code in redirect".to_string())?;

        // Send success response before exchanging (so browser shows result immediately)
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            SUCCESS_HTML.len(),
            SUCCESS_HTML
        );
        let _ = stream.write_all(response.as_bytes()).await;
        drop(stream);

        // Exchange code for tokens (PKCE, no client_secret — Desktop OAuth)
        let redirect_uri = format!("http://localhost:{actual_port}/oauth/callback");
        let http_client = reqwest::Client::new();
        let resp = http_client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("code", code.as_str()),
                ("client_id", client_id.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("grant_type", "authorization_code"),
                ("code_verifier", verifier.as_str()),
            ])
            .send()
            .await
            .map_err(|e| format!("Token exchange failed: {e}"))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Token exchange error: {body}"));
        }

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

        Ok(())
    });

    Ok((handle, actual_port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_code_valid() {
        let req = "GET /oauth/callback?code=4/0AQlEd8x&scope=email HTTP/1.1\r\nHost: localhost\r\n";
        assert_eq!(extract_code(req), Some("4/0AQlEd8x".to_string()));
    }

    #[test]
    fn test_extract_code_missing() {
        let req = "GET /oauth/callback?error=access_denied HTTP/1.1\r\n";
        assert_eq!(extract_code(req), None);
    }

    #[test]
    fn test_extract_code_encoded() {
        let req = "GET /oauth/callback?code=4%2F0AQlEd8x HTTP/1.1\r\n";
        assert_eq!(extract_code(req), Some("4/0AQlEd8x".to_string()));
    }
}
```

- [ ] **Step 2: Add module to oauth/mod.rs**

In `src-tauri/src/oauth/mod.rs`, change from:

```rust
pub mod pkce;
```

to:

```rust
pub mod pkce;
pub mod redirect_server;
```

- [ ] **Step 3: Add urlencoding dependency**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
urlencoding = "2"
```

Note: `urlencoding` is already used in `commands/oauth.rs` — verify it's in Cargo.toml. If not, add it.

- [ ] **Step 4: Run Rust tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && export PATH="$HOME/.cargo/bin:$PATH" && cargo test --lib`
Expected: All tests PASS (including 3 new redirect_server tests)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/oauth/redirect_server.rs src-tauri/src/oauth/mod.rs src-tauri/Cargo.toml
git commit -m "feat(oauth): add one-shot redirect capture server with port fallback"
```

---

### Task 7: Add start_oauth_flow + cancel_oauth_flow commands

**Files:**
- Modify: `src-tauri/src/commands/oauth.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Extend OAuthState with pending_server**

In `src-tauri/src/commands/oauth.rs`, update `OAuthState`:

```rust
#[derive(Default)]
pub struct OAuthState {
    pub pending_verifier: Mutex<Option<String>>,
    pub pending_server: Mutex<Option<tokio::task::JoinHandle<Result<(), String>>>>,
}
```

Add the new result type:

```rust
#[derive(Serialize)]
pub struct OAuthFlowResult {
    pub auth_url: String,
    pub actual_port: u16,
}
```

- [ ] **Step 2: Add start_oauth_flow command**

Add after the existing `exchange_oauth_code` function:

```rust
#[tauri::command]
pub async fn start_oauth_flow(
    engagement_id: String,
    client_id: String,
    redirect_port: u16,
    scopes: Vec<String>,
    state: State<'_, OAuthState>,
    app: AppHandle,
) -> Result<OAuthFlowResult, String> {
    // Cancel any pending flow
    {
        let mut pending = state.pending_server.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = pending.take() {
            handle.abort();
        }
    }

    let challenge = crate::oauth::pkce::generate_pkce();

    // Store verifier
    {
        let mut pending = state.pending_verifier.lock().map_err(|e| e.to_string())?;
        *pending = Some(challenge.verifier.clone());
    }

    // Build keychain key
    let keychain_key = format!("ikrs:{engagement_id}:google");

    // Start redirect server
    let (handle, actual_port) = crate::oauth::redirect_server::start_redirect_server(
        redirect_port,
        client_id.clone(),
        challenge.verifier,
        keychain_key,
        app,
    )
    .await?;

    // Store server handle for cancellation
    {
        let mut pending = state.pending_server.lock().map_err(|e| e.to_string())?;
        *pending = Some(handle);
    }

    // Build auth URL with actual port
    let redirect_uri = format!("http://localhost:{actual_port}/oauth/callback");
    let scope = scopes.join(" ");
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
        client_id={}&\
        redirect_uri={}&\
        response_type=code&\
        scope={}&\
        code_challenge={}&\
        code_challenge_method=S256&\
        access_type=offline&\
        prompt=consent",
        urlencoding::encode(&client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&scope),
        challenge.challenge,
    );

    Ok(OAuthFlowResult {
        auth_url,
        actual_port,
    })
}

#[tauri::command]
pub async fn cancel_oauth_flow(
    state: State<'_, OAuthState>,
) -> Result<(), String> {
    let mut pending = state.pending_server.lock().map_err(|e| e.to_string())?;
    if let Some(handle) = pending.take() {
        handle.abort();
    }
    let mut verifier = state.pending_verifier.lock().map_err(|e| e.to_string())?;
    *verifier = None;
    Ok(())
}
```

Add these imports at the top of `commands/oauth.rs`:

```rust
use tauri::AppHandle;
```

- [ ] **Step 3: Register new commands in lib.rs**

In `src-tauri/src/lib.rs`, add to the `invoke_handler`:

```rust
            commands::oauth::start_oauth_flow,
            commands::oauth::cancel_oauth_flow,
```

(Add after the existing `commands::oauth::exchange_oauth_code` line.)

- [ ] **Step 4: Run Rust check**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && export PATH="$HOME/.cargo/bin:$PATH" && cargo check`
Expected: Compiles without errors

- [ ] **Step 5: Run Rust tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && export PATH="$HOME/.cargo/bin:$PATH" && cargo test --lib`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/oauth.rs src-tauri/src/lib.rs
git commit -m "feat(oauth): add start_oauth_flow + cancel_oauth_flow commands"
```

---

## Wave 3: Frontend Re-Auth + Strict MCP (Tasks 8-9)

### Task 8: Update frontend commands + two-step re-auth in ChatView

**Files:**
- Modify: `src/lib/tauri-commands.ts`
- Modify: `src/views/ChatView.tsx`

- [ ] **Step 1: Add new OAuth commands to tauri-commands.ts**

In `src/lib/tauri-commands.ts`, add after the existing `exchangeOAuthCode` function:

```typescript
export interface OAuthFlowResult {
  auth_url: string;
  actual_port: number;
}

export async function startOAuthFlow(
  engagementId: string,
  clientId: string,
  redirectPort: number,
  scopes: string[],
): Promise<OAuthFlowResult> {
  return invoke("start_oauth_flow", {
    engagementId,
    clientId,
    redirectPort,
    scopes,
  });
}

export async function cancelOAuthFlow(): Promise<void> {
  return invoke("cancel_oauth_flow");
}
```

- [ ] **Step 2: Update spawnClaudeSession to accept strictMcp**

In `src/lib/tauri-commands.ts`, update the existing `spawnClaudeSession`:

```typescript
export async function spawnClaudeSession(
  engagementId: string,
  engagementPath: string,
  resumeSessionId?: string,
  clientSlug?: string,
  strictMcp?: boolean,
): Promise<string> {
  return invoke("spawn_claude_session", {
    engagementId,
    engagementPath,
    resumeSessionId: resumeSessionId ?? null,
    clientSlug: clientSlug ?? null,
    strictMcp: strictMcp ?? null,
  });
}
```

- [ ] **Step 3: Rewrite ChatView re-auth flow**

In `src/views/ChatView.tsx`:

Update imports — replace `startOAuth` with `startOAuthFlow`, add `cancelOAuthFlow`:

```typescript
import { sendClaudeMessage, startOAuthFlow, cancelOAuthFlow, killClaudeSession } from "@/lib/tauri-commands";
```

Add `listen` import:

```typescript
import { listen } from "@tauri-apps/api/event";
```

Get `activeEngagementId` (already available in the component).

Replace the entire `handleReauth` callback:

```typescript
  const handleReauth = useCallback(async () => {
    if (reauthing) return;
    setReauthing(true);
    clearAuthError();
    try {
      const unlisten = await listen<{ keychain_key: string }>(
        "oauth:token-stored",
        async () => {
          unlisten();
          const sid = useClaudeStore.getState().sessionId;
          if (sid) await killClaudeSession(sid);
          await handleConnect();
          setReauthing(false);
        }
      );

      const { auth_url } = await startOAuthFlow(
        activeEngagementId!,
        OAUTH_CLIENT_ID,
        OAUTH_PORT,
        GOOGLE_SCOPES
      );
      await open(auth_url);

      setTimeout(async () => {
        unlisten();
        await cancelOAuthFlow();
        setReauthing(false);
      }, 5 * 60 * 1000);
    } catch (e) {
      useClaudeStore.getState().setError(
        `Re-auth failed: ${e instanceof Error ? e.message : String(e)}`
      );
      setReauthing(false);
    }
  }, [reauthing, clearAuthError, handleConnect, activeEngagementId]);
```

Update the button text for the "waiting" state:

```tsx
{reauthing ? "Waiting for sign-in..." : "Re-authenticate"}
```

- [ ] **Step 4: Run all frontend tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/tauri-commands.ts src/views/ChatView.tsx
git commit -m "feat(oauth): two-step re-auth with redirect server + cancel timeout"
```

---

### Task 9: Strict MCP mode — type, Rust guard, frontend wiring

**Files:**
- Modify: `src/types/index.ts`
- Modify: `src-tauri/src/claude/commands.rs`
- Modify: `src/hooks/useWorkspaceSession.ts`

- [ ] **Step 1: Add strictMcp to Engagement.settings type**

In `src/types/index.ts`, inside the `Engagement` interface's `settings` object, add:

```typescript
    strictMcp?: boolean;
```

(After the `description?: string;` line.)

- [ ] **Step 2: Add strict_mcp parameter to Rust spawn command**

In `src-tauri/src/claude/commands.rs`, update the `spawn_claude_session` signature to add `strict_mcp: Option<bool>` after `client_slug`:

```rust
pub async fn spawn_claude_session(
    engagement_id: String,
    engagement_path: String,
    resume_session_id: Option<String>,
    client_slug: Option<String>,
    strict_mcp: Option<bool>,
    state: State<'_, ClaudeSessionManager>,
    app: AppHandle,
) -> Result<String, String> {
```

Add the validation guard after the token check (after `let has_token = google_token.is_some();`):

```rust
    // Strict MCP: require Google token for fresh spawns (skip on resume — Codex I2)
    if resume_session_id.is_none() && strict_mcp.unwrap_or(false) && !has_token {
        return Err("Strict MCP mode: Google authentication required. Please authenticate before starting this session.".to_string());
    }
```

- [ ] **Step 3: Update all 4 spawnClaudeSession call sites in useWorkspaceSession.ts**

In `src/hooks/useWorkspaceSession.ts`:

**Call site 1** (line 73 — `connect()` primary):
```typescript
      await spawnClaudeSession(engagement.id, engagement.vault.path, resumeId ?? undefined, client?.slug, engagement.settings.strictMcp);
```

**Call site 2** (line 86 — `connect()` fallback):
```typescript
          await spawnClaudeSession(engagement.id, engagement.vault.path, undefined, client?.slug, engagement.settings.strictMcp);
```

**Call site 3** (lines 127-132 — `switchEngagement()` primary):
```typescript
        await spawnClaudeSession(
          newEngagementId,
          engagement.vault.path,
          resumeId ?? undefined,
          switchClient?.slug,
          engagement.settings.strictMcp,
        );
```

**Call site 4** (line 142 — `switchEngagement()` fallback):
```typescript
            await spawnClaudeSession(newEngagementId, engagement.vault.path, undefined, switchClient?.slug, engagement.settings.strictMcp);
```

- [ ] **Step 4: Run Rust tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && export PATH="$HOME/.cargo/bin:$PATH" && cargo test --lib`
Expected: All tests PASS

- [ ] **Step 5: Run frontend tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/types/index.ts src-tauri/src/claude/commands.rs src/hooks/useWorkspaceSession.ts src/lib/tauri-commands.ts
git commit -m "feat(mcp): add strict MCP mode — block spawn without Google auth"
```

---

## Wave 4: Verification + Docs

### Task 10: Full verification + spec status update

- [ ] **Step 1: Run all Rust tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && export PATH="$HOME/.cargo/bin:$PATH" && cargo test --lib`
Expected: All tests PASS

- [ ] **Step 2: Run all frontend tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run`
Expected: All tests PASS

- [ ] **Step 3: Cargo check (full compile)**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && export PATH="$HOME/.cargo/bin:$PATH" && cargo check`
Expected: Compiles without errors

- [ ] **Step 4: Grep for orphans**

Run: `grep -r "startOAuth\b" src/ --include="*.ts" --include="*.tsx"` — should return 0 results (replaced by startOAuthFlow)
Run: `grep -r "McpProcessManager" src-tauri/src/` — should return 0 results

- [ ] **Step 5: Update spec status**

In `docs/specs/m2-phase3c-mcp-polish-design.md`, change `Status: Draft` to `Status: Complete`.

- [ ] **Step 6: Commit**

```bash
git add docs/specs/m2-phase3c-mcp-polish-design.md
git commit -m "docs: mark Phase 3c spec complete"
```

---

## Summary

| Wave | Tasks | Description |
|------|-------|-------------|
| 1 | 1-4 | MCP health wiring + all frontend tests (TDD) |
| 2 | 5-7 | Auth-error fix + OAuth redirect server + commands |
| 3 | 8-9 | Frontend re-auth rewrite + strict MCP mode |
| 4 | 10 | Verification + docs |

**Total commits:** 10
**Codex checkpoints:** After Wave 2 (mid) and after Wave 4 (final)
