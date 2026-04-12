# M2 Phase 3c: MCP Polish + Test Coverage

**Status:** Draft
**Date:** 2026-04-12
**Parent spec:** `embedded-claude-architecture.md` (Phase 3)
**Prior phase:** `m2-phase3b-mcp-wiring-design.md` (complete, Codex 9/10)
**Codex reviews:** WARN 7/10 → C1/C2/I1-I4/N1-N4 addressed below

---

## Goal

Close all deferred Phase 3 items: populate MCP server health from the session-ready event, fix the re-auth timing gap, add strict MCP mode for NDA engagements, and add frontend test coverage for MCP-related state and UI.

## Scope

### In Scope

1. MCP server health indicators from `system.init` tools list
2. Two-step re-auth flow with OAuth callback awareness
3. `strictMcpConfig` engagement setting
4. Frontend tests for MCP store, auth-error state, and re-auth flow

### Out of Scope (Phase 4)

- Offline detection and retry UX
- `npx` → pre-resolved binary paths optimization
- DMG packaging, code signing, sandboxing

---

## Design

### 1. MCP Server Health from system.init

**Problem:** After Phase 3b retired `McpProcessManager`, `mcpStore.setServers()` is never called. Consumer hooks (`useGmail`, `useDrive`, `useCalendar`, `useNotes`) all check `isConnected` via mcpStore but always see empty servers.

**Solution:** When `claude:session-ready` fires, the `tools` array contains MCP tool names with prefixes like `mcp__gmail__read_message`. Parse these to infer which MCP servers connected, then populate `mcpStore`.

**Implementation:**

In `useClaudeStream.ts`, inside the existing `claude:session-ready` listener, add a call to populate mcpStore:

```typescript
// After store().setSessionReady(...)
const mcpServers = extractMcpServers(event.payload.tools);
useMcpStore.getState().setServers(mcpServers);
```

New utility function `extractMcpServers` in `src/lib/mcp-utils.ts`:

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
    // MCP tools follow pattern: mcp__{server}__{method}
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
    restartCount: 0,  // Codex N1: satisfy McpHealth.restartCount (required field)
  }));
}
```

**On session disconnect:** Clear mcpStore servers so consumer hooks see `isConnected: false`.

Per Codex N2, do NOT add cross-store calls inside `claudeStore.setDisconnected`. Instead, clear mcpStore from the `claude:session-ended` listener in `useClaudeStream.ts`, keeping store coupling in the event-wiring layer:

```typescript
// In useClaudeStream.ts, inside claude:session-ended listener
useMcpStore.getState().setServers([]);
```

**McpHealth type** (already exists in `src/types/index.ts`):
```typescript
export interface McpHealth {
  type: McpServerType;
  status: McpHealthStatus;
  pid?: number;        // Unused after McpProcessManager retirement — omit (undefined)
  lastPing?: Date;
  restartCount: number; // Set to 0 for MCP servers inferred from tools list
}
```

The `extractMcpServers` function must populate `restartCount: 0` and omit `pid` to satisfy the existing interface.

### 2. Two-Step Re-Auth Flow

**Problem:** The current `handleReauth` in ChatView opens the OAuth browser and immediately kills the session + reconnects. The user hasn't completed the OAuth consent yet, so the reconnected session uses the old expired token.

**Discovery:** No OAuth redirect capture server exists. The M1 PKCE flow built `start_oauth` (generates auth URL with PKCE challenge) and `exchange_oauth_code` (exchanges code for tokens), but nothing listens on `http://localhost:{port}/oauth/callback` to capture the redirect. This means re-auth (and initial OAuth) cannot complete end-to-end.

**OAuth client type (Codex C1):** This app uses a **Desktop** type OAuth client ID from Google Cloud Console. Desktop apps use PKCE without a `client_secret` — the existing `exchange_oauth_code` in `commands/oauth.rs` correctly omits it. This is intentional and spec-compliant for Tauri desktop apps.

**Solution:** Add a lightweight one-shot HTTP server in Rust that:
1. Tries to bind `localhost:{port}` — if port taken, tries `port+1` through `port+10` (Codex I4: port collision)
2. Waits for Google's redirect with the auth `code` parameter
3. Calls the existing PKCE token exchange logic internally (no `client_secret` — Desktop OAuth)
4. Stores the token in the keychain via `KeyringExt`
5. Emits `oauth:token-stored` Tauri event
6. Serves a "Sign-in complete, you can close this tab" HTML response
7. Shuts down

**New Rust module:** `src-tauri/src/oauth/redirect_server.rs`

```rust
/// Spawns a one-shot HTTP server on localhost:{port} to capture the OAuth redirect.
/// Tries ports port..port+10 if the first is occupied (Codex I4).
/// Returns (JoinHandle, actual_port) so the auth URL uses the correct redirect_uri.
pub async fn start_redirect_server(
    preferred_port: u16,
    client_id: String,
    verifier: String,
    keychain_key: String,
    app: AppHandle,
) -> Result<(tokio::task::JoinHandle<Result<(), String>>, u16), String>
```

Uses `tokio::net::TcpListener` + minimal HTTP parsing (no framework needed — single GET request with query params). The server handles exactly one request and shuts down.

**Cancellation (Codex I3):** The `OAuthState` struct is extended to store the pending server handle:

```rust
pub struct OAuthState {
    pub pending_verifier: Mutex<Option<String>>,
    pub pending_server: Mutex<Option<tokio::task::JoinHandle<Result<(), String>>>>,
}
```

**New Tauri commands:**

`start_oauth_flow` — combines `start_oauth` + `start_redirect_server` into one atomic operation:

```rust
#[tauri::command]
pub async fn start_oauth_flow(
    engagement_id: String,
    client_id: String,
    redirect_port: u16,
    scopes: Vec<String>,
    state: State<'_, OAuthState>,
    app: AppHandle,
) -> Result<OAuthFlowResult, String>
// Returns { auth_url, actual_port } — actual_port may differ from redirect_port
```

`cancel_oauth_flow` — aborts the redirect server if user cancels or timeout fires (Codex I3):

```rust
#[tauri::command]
pub async fn cancel_oauth_flow(
    state: State<'_, OAuthState>,
) -> Result<(), String>
// Aborts pending_server JoinHandle, clears pending_verifier
```

This generates the PKCE challenge, starts the redirect server, and returns the auth URL with the actual bound port. The redirect server runs in the background and emits `oauth:token-stored` when done.

**ChatView re-auth flow (updated):**

```typescript
const handleReauth = useCallback(async () => {
  if (reauthing) return;
  setReauthing(true);
  clearAuthError();
  try {
    // Listen for token stored event BEFORE starting flow
    const unlisten = await listen<{ engagement_id: string }>(
      "oauth:token-stored",
      async () => {
        unlisten();
        // Token is now in keychain — safe to reconnect
        const sid = useClaudeStore.getState().sessionId;
        if (sid) await killClaudeSession(sid);
        await handleConnect();
        setReauthing(false);
      }
    );

    // Start OAuth flow (starts redirect server + returns auth URL)
    const { auth_url } = await startOAuthFlow(
      engagementId, OAUTH_CLIENT_ID, OAUTH_PORT, GOOGLE_SCOPES
    );
    await open(auth_url);

    // Safety timeout: 5 minutes — cancel redirect server on expiry (Codex I3)
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
}, [reauthing, clearAuthError, handleConnect]);
```

**UX change:** Button shows "Waiting for sign-in..." after clicking. Only reconnects once the redirect server captures the token.

**Frontend command updates** in `tauri-commands.ts` (Codex N4: update import in ChatView too):

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
    engagementId, clientId, redirectPort, scopes,
  });
}

export async function cancelOAuthFlow(): Promise<void> {
  return invoke("cancel_oauth_flow");
}
```

ChatView import line changes: remove `startOAuth`, add `startOAuthFlow` and `cancelOAuthFlow`.

### 2b. Fix auth-error server inference (Codex C2 — pre-existing bug)

**Problem:** In `stream_parser.rs`, `infer_mcp_server()` is called with `tool_use_id` (opaque ID like `toolu_01A2B3`), not the tool name (like `mcp__gmail__read_message`). The substring matching (`contains("gmail")`) never matches because `tool_use_id` values don't contain server names.

**Fix:** Track a `tool_id → tool_name` mapping. When `claude:tool-start` fires (in `handle_assistant_event`), store `tool_id → tool_name`. When `handle_user_event` processes a `tool_result`, look up the tool name from the map.

Add to `parse_stream`:
```rust
let mut tool_name_map: HashMap<String, String> = HashMap::new();
```

Pass `&mut tool_name_map` to `handle_assistant_event` (to insert) and `handle_user_event` (to lookup).

In `handle_assistant_event`, after emitting `claude:tool-start`:
```rust
tool_name_map.insert(tool_id.to_string(), tool_name.to_string());
```

In `handle_user_event`, replace `infer_mcp_server(tool_id)` with:
```rust
let tool_name = tool_name_map.get(tool_id).map(|s| s.as_str()).unwrap_or("");
let server = infer_mcp_server(tool_name);  // Now matches mcp__gmail__* correctly
```

Update `infer_mcp_server` to match on tool name prefix patterns (same `mcp__gmail__` pattern as `extractMcpServers`).

### 3. Strict MCP Config

**Problem:** NDA clients may require that specific MCP servers are available. Currently, missing servers degrade silently (Gmail absent = no email tools, but session still starts).

**Solution:** Add optional `strictMcp` setting to the Engagement type. When enabled, the Rust spawn command validates that the MCP config includes all required servers before spawning.

**Type change** in `src/types/index.ts`:

```typescript
export interface Engagement {
  // ... existing fields
  settings: {
    timezone: string;
    billingRate?: number;
    description?: string;
    strictMcp?: boolean;  // When true, require Google token for MCP servers
  };
  // ...
}
```

**Rust command change** in `src-tauri/src/claude/commands.rs`:

Add `strict_mcp: Option<bool>` parameter. When `true` and `has_token` is `false` AND this is a fresh spawn (not resume), return an error (Codex I2: skip validation on resume since the session already had MCP context):

```rust
// Only enforce on fresh spawns — resumed sessions already have MCP context (Codex I2)
if resume_session_id.is_none() && strict_mcp.unwrap_or(false) && !has_token {
    return Err("Strict MCP mode: Google authentication required. Please authenticate before starting this session.".to_string());
}
```

**Frontend:** `useWorkspaceSession.ts` reads `engagement.settings.strictMcp` and passes it to `spawnClaudeSession`. All 4 existing call sites add the new param (Codex N3):

```typescript
await spawnClaudeSession(
  engagement.id, engagement.vault.path,
  resumeId ?? undefined, client?.slug,
  engagement.settings.strictMcp,  // NEW — undefined if not set
);
```

**Tauri command signature:**
```rust
pub async fn spawn_claude_session(
    engagement_id: String,
    engagement_path: String,
    resume_session_id: Option<String>,
    client_slug: Option<String>,
    strict_mcp: Option<bool>,  // NEW — None/false = graceful degradation
    state: State<'_, ClaudeSessionManager>,
    app: AppHandle,
) -> Result<String, String>
```

**TS command signature** in `tauri-commands.ts`:
```typescript
export async function spawnClaudeSession(
  engagementId: string, engagementPath: string,
  resumeSessionId?: string, clientSlug?: string,
  strictMcp?: boolean,  // NEW
): Promise<string> {
  return invoke("spawn_claude_session", {
    engagementId, engagementPath,
    resumeSessionId: resumeSessionId ?? null,
    clientSlug: clientSlug ?? null,
    strictMcp: strictMcp ?? null,
  });
}
```

### 4. Frontend Tests

**Framework:** Vitest + `@testing-library/react` (configured in `vitest.config.ts`)
**Location:** `tests/unit/` (follows existing convention)

**Prerequisite (Codex I1):** Create `tests/setup.ts` with Tauri API mocks since stores and hooks import `@tauri-apps/api/event` and `@tauri-apps/api/core`:

```typescript
// tests/setup.ts
import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// Mock Tauri core invoke
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// Mock Tauri event system
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));

// Mock Tauri plugin opener
vi.mock("@tauri-apps/plugin-opener", () => ({
  open: vi.fn(),
}));
```

**Test files to create:**

1. **`tests/unit/lib/mcp-utils.test.ts`** — unit tests for `extractMcpServers`:
   - Extracts gmail, calendar, drive, obsidian from mixed tool list
   - Ignores non-MCP tools (Read, Write, Edit, etc.)
   - Deduplicates (multiple gmail tools → one entry)
   - Returns empty array for no MCP tools
   - Ignores unknown MCP prefixes
   - Each result has `restartCount: 0` and `status: "healthy"`

2. **`tests/unit/stores/mcpStore.test.ts`** — store tests:
   - `setServers` populates server list
   - `setServerHealth` updates individual server status
   - Consumer hooks see correct `isConnected` after `setServers`

3. **`tests/unit/stores/claudeStore-auth.test.ts`** — auth-error state:
   - `setAuthError` stores server + hint
   - `clearAuthError` resets to null
   - `reset()` clears authError
   - `setDisconnected` clears sessions (existing, verify no regression)

---

## Success Criteria

1. After session connects with Gmail MCP tools, `useGmail().isConnected` returns `true`
2. After session disconnects, all MCP servers show `isConnected: false`
3. Re-auth flow waits for OAuth completion before reconnecting
4. Strict MCP mode blocks session spawn when Google token is missing
5. All new tests pass (`npx vitest run`)
6. All existing Rust tests pass (`cargo test --lib`)
7. Zero new `any` types, TODOs, or placeholder code

---

## Files Changed

| File | Action | Description |
|------|--------|-------------|
| `src/lib/mcp-utils.ts` | CREATE | `extractMcpServers` utility |
| `src/hooks/useClaudeStream.ts` | MODIFY | Wire session-ready → mcpStore, clear on session-ended (N2) |
| `src/views/ChatView.tsx` | MODIFY | Two-step re-auth with event listener + cancel (N4: update imports) |
| `src/types/index.ts` | MODIFY | Add `strictMcp?` to Engagement.settings |
| `src/lib/tauri-commands.ts` | MODIFY | Add `startOAuthFlow`, `cancelOAuthFlow`, `strictMcp` param |
| `src/hooks/useWorkspaceSession.ts` | MODIFY | Pass strictMcp to all 4 spawn call sites (N3) |
| `src-tauri/src/oauth/redirect_server.rs` | CREATE | One-shot HTTP server for OAuth redirect capture |
| `src-tauri/src/oauth/mod.rs` | MODIFY | Add `pub mod redirect_server;` |
| `src-tauri/src/commands/oauth.rs` | MODIFY | Add `start_oauth_flow` + `cancel_oauth_flow`, extend `OAuthState` |
| `src-tauri/src/claude/commands.rs` | MODIFY | Add `strict_mcp` param + validation (skip on resume, I2) |
| `src-tauri/src/claude/stream_parser.rs` | MODIFY | Fix tool_id→tool_name mapping for auth-error detection (C2) |
| `src-tauri/src/lib.rs` | MODIFY | Register `start_oauth_flow` + `cancel_oauth_flow` commands |
| `tests/setup.ts` | CREATE | Tauri API mocks for Vitest (I1) |
| `tests/unit/lib/mcp-utils.test.ts` | CREATE | extractMcpServers tests |
| `tests/unit/stores/mcpStore.test.ts` | CREATE | mcpStore tests |
| `tests/unit/stores/claudeStore-auth.test.ts` | CREATE | Auth-error state tests |
