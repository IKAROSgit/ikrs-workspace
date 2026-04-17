# M2 Phase 3b: MCP Wiring + Token Resilience

**Status:** Complete
**Date:** 2026-04-12 (amended 2026-04-17)
**Parent spec:** `embedded-claude-architecture.md` (Phase 3)
**Prior phase:** `m2-phase3a-session-ux-design.md` (complete)
**Codex reviews:**
- Option A approved (2026-04-12, pre-impl design)
- Config design WARN 7/10 (C1/I1/I2 addressed in-spec)
- Retroactive final review PASS 9/10 (`2026-04-16-m2-phase3b-final-review.md`)
- Second-opinion sign-off PASS WITH CONDITIONS 8/10 (`2026-04-17-m2-phase3b-spec-signoff.md`) — conditions closed via this 2026-04-17 amendment (risk table updated with 4a/3c closures; Section 5 re-auth flow aligned with shipped event-driven implementation)

---

## Goal

Wire MCP servers (Gmail, Calendar, Drive, Obsidian) into Claude CLI sessions via `--mcp-config`, retire the app-side `McpProcessManager`, and handle Google token expiry gracefully.

## Architectural Decision

**Claude CLI owns MCP server lifecycle** (Option A — Codex-approved).

Rationale: The current `McpProcessManager` spawns MCP servers as independent child processes that Claude cannot see or use. The `--mcp-config` flag lets Claude CLI spawn and manage MCP servers as its own children, giving it native tool access. MCP servers are stateless API bridges — they hold no session state, so restart cost is limited to startup latency (mitigated by future binary path pre-resolution).

Alternatives rejected:
- **App spawns + discovery bridge (B):** Claude CLI has no mechanism to connect to existing MCP processes by PID. Would require building a custom bridge layer — violates Golden Rule #9.
- **Hybrid SSE proxy (C1):** Adds a proxy layer between Claude and MCP servers. More failure modes, zero benefit since stdio is natively supported.
- **Double spawning (C3):** App spawns for health monitoring, Claude spawns for usage. Two copies of each server — wasteful, confusing, impossible to keep in sync.

## Scope

### In scope
1. Per-engagement `.mcp-config.json` generation at session spawn time
2. `--mcp-config` flag and env var injection in `session_manager.rs`
3. Gmail, Calendar, Drive MCP wiring (conditional on Google token existence)
4. Obsidian MCP wiring (conditional on vault path existence)
5. `McpProcessManager` full retirement (Rust module + Tauri commands + frontend hooks)
6. Google token expiry detection and re-auth flow
7. Vault directory creation preserved in new flow (Codex C1 fix)

### Out of scope (Phase 3c+)
- Offline detection and retry UX
- `--strict-mcp-config` as engagement-level setting (NDA clients)
- `npx` → pre-resolved binary paths optimization (Phase 4 distribution concern)
- MCP server health indicators in UI from `system.init` parsing
- Component-level tests for MCP UI elements

---

## Design

### 1. MCP Config Generation

At session spawn time, the Rust backend generates `.mcp-config.json` in the engagement directory.

**Config structure:**

```json
{
  "mcpServers": {
    "gmail": {
      "command": "npx",
      "args": ["@shinzolabs/gmail-mcp@1.7.4"],
      "env": { "GOOGLE_ACCESS_TOKEN": "${GOOGLE_ACCESS_TOKEN}" }
    },
    "calendar": {
      "command": "npx",
      "args": ["@cocal/google-calendar-mcp@2.6.1"],
      "env": { "GOOGLE_ACCESS_TOKEN": "${GOOGLE_ACCESS_TOKEN}" }
    },
    "drive": {
      "command": "npx",
      "args": ["@piotr-agier/google-drive-mcp@2.0.2"],
      "env": { "GOOGLE_ACCESS_TOKEN": "${GOOGLE_ACCESS_TOKEN}" }
    },
    "obsidian": {
      "command": "npx",
      "args": ["@bitbonsai/mcpvault@1.3.0", "/resolved/vault/path"],
      "env": {}
    }
  }
}
```

**Resolution rules:**
- `${GOOGLE_ACCESS_TOKEN}` is an env var placeholder — Claude CLI resolves it from its process environment at MCP server spawn time. The actual token is never written to disk.
- `{vaultPath}` is resolved at generation time to an absolute path from the client slug: `~/.ikrs-workspace/vaults/{client_slug}`.
- Gmail, Calendar, Drive entries are included only if a Google OAuth token exists in OS keychain for the engagement (expired tokens are included — Claude reports errors, user sees "re-auth needed" rather than servers silently missing).
- Obsidian entry is included only if the vault directory exists on disk.
- Config uses `--mcp-config` (additive), not `--strict-mcp-config` — preserves consultant's personal MCP servers.

**Generation location:** Rust function `generate_mcp_config()` in a new `src-tauri/src/claude/mcp_config.rs` module.

### 2. Spawn Flow Changes

`session_manager.rs` `spawn()` signature extends:

```rust
pub async fn spawn(
    &self,
    engagement_id: String,
    engagement_path: String,
    resume_session_id: Option<String>,
    env_vars: HashMap<String, String>,
    mcp_config_path: Option<String>,
    app: AppHandle,
) -> Result<(String, u32), String>
```

Changes to spawn:
- `.envs(&env_vars)` on the `Command` builder injects env vars into the Claude CLI subprocess
- When `mcp_config_path` is `Some(path)`, adds `--mcp-config {path}` to CLI args
- No changes to stream parsing, monitoring, or registry logic

### 3. Tauri Command Orchestration

`spawn_claude_session` in `commands.rs` becomes the orchestrator. Its Tauri command signature adds a `client_slug: String` parameter (passed from the frontend, which already has it from `engagementStore.clients`):

```rust
pub async fn spawn_claude_session(
    engagement_id: String,
    engagement_path: String,
    resume_session_id: Option<String>,
    client_slug: String,  // NEW — needed for vault path + keychain key
    state: State<'_, ClaudeSessionManager>,
    app: AppHandle,
) -> Result<String, String>
```

Orchestration steps:
```
1. Read Google OAuth token from OS keychain using key format "ikrs:{engagement_id}:google"
2. Resolve vault path: ~/.ikrs-workspace/vaults/{client_slug}
3. Create vault directory if it does not exist (preserves createVault lifecycle — Codex C1)
4. Call generate_mcp_config(engagement_path, token.is_some(), vault_path)
5. Build env_vars HashMap (GOOGLE_ACCESS_TOKEN → token, if present)
6. Call session_manager.spawn(..., env_vars, Some(mcp_config_path), ...)
7. Register session in registry (unchanged)
```

The TypeScript wrapper `spawnClaudeSession` in `tauri-commands.ts` adds the `clientSlug` parameter:

```typescript
export async function spawnClaudeSession(
  engagementId: string,
  engagementPath: string,
  resumeSessionId?: string,
  clientSlug?: string,
): Promise<string>
```

### 4. McpProcessManager Retirement

**Files to delete:**
- `src-tauri/src/mcp/manager.rs` — McpProcessManager struct, spawn/kill/health_check/restart
- `src-tauri/src/mcp/mod.rs` — McpServerType enum, re-exports
- `src-tauri/src/commands/mcp.rs` — Tauri command handlers (spawn_mcp, kill_mcp, kill_all_mcp, mcp_health, restart_mcp)

**Files to modify:**
- `src-tauri/src/lib.rs` — remove `mod mcp;` declaration, remove `McpProcessManager` state `.manage()` call, remove 5 MCP command registrations from invoke_handler
- `src-tauri/src/commands/mod.rs` — remove `pub mod mcp;` declaration
- `src/hooks/useEngagement.ts` — delete entirely (vault creation moves to Rust, MCP spawning eliminated)
- `src/lib/tauri-commands.ts` — remove spawnMcp, killMcp, killAllMcp, mcpHealth, restartMcp; update spawnClaudeSession signature to add clientSlug param

**No changes needed:**
- `src/stores/mcpStore.ts` — no Tauri invoke calls to remove; store naturally becomes inert when useEngagement.ts is deleted (its only consumer). Keep for future MCP status display from Claude's stream.
- `src/hooks/useWorkspaceSession.ts` — does not reference refreshMcpServers; no changes needed for MCP retirement.
- `src/types/index.ts` — McpServerType, McpHealthStatus, McpHealth types are preserved as-is for future UI use.

**Types preserved (no move needed):**
- `McpServerType` stays in frontend types (used for UI status display)
- `McpHealth` / `McpHealthStatus` stays in frontend types

### 5. Token Expiry Detection and Re-Auth Flow

**Detection:** The stream parser (`stream_parser.rs`) watches for MCP tool error patterns in Claude's output. When Claude reports an MCP tool failure containing auth-related keywords (`401`, `403`, `token expired`, `authentication failed`, `invalid_grant`, `UNAUTHENTICATED`), the parser emits a `claude:mcp-auth-error` Tauri event.

**Event payload:**
```rust
struct McpAuthErrorPayload {
    server_name: String,  // "gmail", "calendar", "drive"
    error_hint: String,   // extracted error message
}
```

**Frontend flow** (as shipped — amended 2026-04-17 to reflect event-driven implementation):
1. `useClaudeStream` listens for `claude:mcp-auth-error`
2. Sets `claudeStore.authError = { server, hint }` state
3. `ChatView` renders a non-blocking toast/banner: "Google authentication expired. Re-authenticate to restore {server}."
4. Toast button invokes `startOAuthFlow()` (unified OAuth command from Phase 4a redirect-server refactor, not a direct Google URL trigger). A 5-minute `cancelOAuthFlow()` timer arms in case the user abandons the consent screen.
5. Rust backend completes OAuth, writes fresh access + refresh tokens to the keychain, and emits a `oauth:token-stored` Tauri event.
6. `ChatView`'s event listener for `oauth:token-stored` chains: (a) kill current Claude session, (b) call `handleConnect()` which respawns via `useWorkspaceSession`, injecting a freshly-refreshed token via `session_manager.spawn(..., env_vars, ...)`. Session resume preserves conversation.

The flow is **event-driven, not synchronous** — the app never polls or awaits the OAuth result inline. This matters for (a) UX responsiveness, (b) interop with Phase 4a's token refresh module (`refresh_if_needed` runs on every spawn, so an expired-but-refreshable token self-heals without triggering this path at all), and (c) cancellation — if the user closes the consent tab, the timeout fires a clean state reset.

**Graceful degradation:** Claude keeps working — Obsidian, chat, and other tools are unaffected. Only Google-dependent tools fail. The user can choose to re-auth immediately or continue without Google tools.

**Non-goal:** Automatic token refresh without user interaction. OAuth token refresh typically requires user consent or a refresh token (which these MCP packages may not support). The re-auth flow is explicit and user-initiated.

---

## File Change Summary

| File | Action | Description |
|------|--------|-------------|
| `src-tauri/src/claude/mcp_config.rs` | CREATE | Config generation: generate_mcp_config(), MCP server definitions |
| `src-tauri/src/claude/mod.rs` | MODIFY | Add mcp_config module declaration |
| `src-tauri/src/claude/session_manager.rs` | MODIFY | Add env_vars + mcp_config_path params to spawn() |
| `src-tauri/src/claude/commands.rs` | MODIFY | Add client_slug param, orchestrate: keychain read, vault create, config gen, spawn |
| `src-tauri/src/claude/stream_parser.rs` | MODIFY | Add auth-error pattern detection, emit claude:mcp-auth-error |
| `src-tauri/src/claude/types.rs` | MODIFY | Add McpAuthErrorPayload |
| `src-tauri/src/mcp/manager.rs` | DELETE | McpProcessManager retired |
| `src-tauri/src/mcp/mod.rs` | DELETE | MCP module retired |
| `src-tauri/src/commands/mcp.rs` | DELETE | MCP Tauri commands retired |
| `src-tauri/src/commands/mod.rs` | MODIFY | Remove `pub mod mcp;` declaration |
| `src-tauri/src/lib.rs` | MODIFY | Remove `mod mcp;`, McpProcessManager state, 5 MCP commands from invoke_handler |
| `src/hooks/useEngagement.ts` | DELETE | MCP spawning + vault creation moved to Rust |
| `src/lib/tauri-commands.ts` | MODIFY | Remove MCP command wrappers, add clientSlug to spawnClaudeSession |
| `src/stores/claudeStore.ts` | MODIFY | Add authError state + setter |
| `src/hooks/useClaudeStream.ts` | MODIFY | Listen for claude:mcp-auth-error |
| `src/views/ChatView.tsx` | MODIFY | Render auth-error toast with re-auth button |
| `src/types/claude.ts` | MODIFY | Add McpAuthErrorPayload type |
| `docs/specs/embedded-claude-architecture.md` | MODIFY | Amend Phase 3 Q3: "scaffold time" → "spawn time" (Codex I1) |

---

## Codex Findings Addressed

| ID | Finding | Resolution |
|----|---------|------------|
| C1 | Vault directory creation orphaned when useEngagement.ts retired | Vault creation moved to Rust-side command orchestrator (Section 3, step 3) |
| I1 | Spec says "scaffold time" but design uses "spawn time" | This spec codifies spawn-time generation; parent spec Q3 needs amendment |
| I2 | spawn() has no env var injection or --mcp-config flag | spawn() signature extended with env_vars and mcp_config_path (Section 2) |

## Risks

| Risk | Impact | Mitigation | Status (as of 2026-04-17) |
|------|--------|------------|---------------------------|
| npx cold start latency (5-15s per server) | First session for an engagement takes 20-60s for MCP init | Document as known limitation. Phase 4: pre-resolve to binary paths | **Closed in Phase 4a** (`5580ed2`, `b89e820`) — binary resolver locates `claude`/`npx`/`node` without PATH dependency |
| Token expiry mid-session | Google MCP tools fail until re-auth | Auth-error detection + re-auth toast (Section 5) | Open — shipped in `aa2a433`. Additional defence-in-depth added in Phase 4a via `oauth::token_refresh::refresh_if_needed` running on every spawn |
| macOS App Sandbox blocks npx (Phase 4) | MCP servers fail to spawn on distributed app | Pre-resolved binary paths eliminate npx dependency | **Closed in Phase 4a** (same binary resolver work) |
| .mcp-config.json readable by Claude | Claude can see package names/versions | No secrets in file — token passed via env var only | Closed — confirmed by post-ship Codex audit; `.mcp-config.json` added to `.gitignore` in `66dfb10` |
| Consultant's personal MCP servers conflict | Additive mode may load unexpected servers | Future: `--strict-mcp-config` as engagement setting | **Delivered in Phase 3c** (`02c708b`) — strict MCP mode available as engagement-level setting |
| Resume after mid-turn kill for re-auth | `--resume` may not recover cleanly if killed mid-tool-use | Accepted — user can re-ask; conversation context preserved even if last turn is incomplete | Open — accepted posture unchanged |
| `infer_mcp_server` keyed on opaque `tool_id` | Stream parser always reported `"unknown"` for auth errors, so re-auth toast could not tell user which server failed | Re-key to `tool_name` via `tool_name_map` | **Closed in Phase 3c** (`26dbb71`, Codex C2) — latent 3b bug caught post-ship, recorded here for provenance |

## Success Criteria

1. Claude CLI spawns with `--mcp-config` pointing to per-engagement config
2. Gmail, Calendar, Drive tools available in Claude session when Google token exists
3. Obsidian tools available when vault exists
4. McpProcessManager fully removed — zero orphan code
5. Token expiry produces user-visible re-auth prompt, not silent failures
6. Session resume works with MCP config (resume loads same MCP servers)
7. Engagement switching regenerates MCP config for new engagement
