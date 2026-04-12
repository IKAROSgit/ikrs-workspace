# Phase 4a: macOS Sandbox Readiness + Code Signing

**Status:** Draft
**Parent spec:** `embedded-claude-architecture.md` (Phase 4: Distribution)
**Approach:** B -- macOS signed + notarized .dmg; unsigned Linux/Windows stubs from existing CI
**Phase split:** 4a (this spec) = sandbox + signing. 4b (later) = DMG polish, auto-update, offline graceful degradation.
**Codex reviews:** Scope WARN 7/10 (2026-04-12), all 6 conditions addressed below.

## 1. Goal

Produce a signed, notarized macOS `.dmg` that:
- Installs without Gatekeeper warnings on any macOS 12+ machine
- Spawns Claude CLI and MCP servers correctly under App Sandbox
- Persists workspace folder access across app restarts via Security-Scoped Bookmarks
- Handles OAuth token refresh (no re-auth every hour)

Non-macOS platforms continue shipping as unsigned CI artifacts (.deb, .AppImage, .msi).

## 2. Out of Scope

- DMG background image / visual customization (4b)
- Auto-update via tauri-plugin-updater (4b)
- Offline detection + graceful degradation UI (4b)
- Windows code signing (future)
- App Store submission (future)
- Linux signing (no centralized authority)

## 3. Prerequisites (Phase 3 Debt)

These must be resolved before sandbox work begins. Order matters (Codex W3).

### 3.1 P1: App Identifier Change (Codex A2 + W4)

**Current:** `com.moe_ikaros_ae.ikrs-workspace` (underscores, Apple rejects for notarization)
**Target:** `ae.ikaros.workspace` (reverse-domain, matches ikaros.ae)

**Files changed:**
- `src-tauri/tauri.conf.json` line 5

**Data migration:**
On macOS, Tauri derives the app data directory from the identifier:
- Old: `~/Library/Application Support/com.moe_ikaros_ae.ikrs-workspace/`
- New: `~/Library/Application Support/ae.ikaros.workspace/`

Add a one-time migration check in `lib.rs` setup hook: if old directory exists and new does not, move `session-registry.json` and any SQLite databases from old to new. Log the migration. This only affects developers who ran pre-distribution builds.

### 3.2 P2: OAuth Refresh Token Storage (Codex B2 + C1)

**Problem:** `redirect_server.rs` (line 104) stores only `access_token` in the keychain. Google access tokens expire after 1 hour. Without `refresh_token`, users must re-authenticate every hour -- unacceptable for distribution.

**Solution: JSON token payload in keychain**

Keychain value changes from plain string to:
```json
{
  "access_token": "ya29...",
  "refresh_token": "1//0e...",
  "expires_at": 1744567890
}
```

**Propagation chain:**

| File | Change |
|------|--------|
| `src-tauri/src/oauth/redirect_server.rs` | Extract `refresh_token` and `expires_in` from Google's token response. Compute `expires_at = now + expires_in`. Store JSON blob in keychain. |
| `src-tauri/src/oauth/token_refresh.rs` | **New module.** `refresh_if_needed(keychain_key: &str, client_id: &str, app: &AppHandle) -> Result<String, String>`. Reads JSON from keychain, checks `expires_at` against current time (with 5-minute buffer). If expired, calls `https://oauth2.googleapis.com/token` with `grant_type=refresh_token`. Updates keychain with new access_token + expires_at. Returns valid access_token. |
| `src-tauri/src/claude/commands.rs` | Replace direct keychain read with `token_refresh::refresh_if_needed()` before injecting `GOOGLE_ACCESS_TOKEN` env var into the Claude session. |
| `src-tauri/src/oauth/mod.rs` | Add `pub mod token_refresh;` |
| Frontend (`tauri-commands.ts`) | No change -- token handling is entirely Rust-side. |

**Token refresh flow at session spawn:**
```
spawn_claude_session()
  -> refresh_if_needed(keychain_key, client_id, app)
     -> read JSON from keychain
     -> if expires_at > now + 300s: return access_token (still valid)
     -> else: POST /token with refresh_token
        -> on success: update keychain JSON, return new access_token
        -> on failure: return Err("Google session expired. Please re-authenticate.")
  -> inject refreshed token as GOOGLE_ACCESS_TOKEN env var
  -> spawn Claude CLI
```

**Google OAuth client_id for Rust-side refresh:** The client_id is needed by `token_refresh.rs` to call Google's token endpoint. Rather than adding another parameter to `spawn_claude_session`, embed the client_id in the JSON token payload stored in the keychain (alongside access_token, refresh_token, expires_at). The redirect server already has client_id when it performs the initial exchange -- store it at that point. This keeps the spawn command signature stable and avoids frontend-to-backend plumbing for a value that doesn't change per-session.

### 3.3 P3: SettingsView OAuth Migration (Codex B1 + C2)

**Problem:** `SettingsView.tsx` (line 122) uses old `startOAuth()` which only generates an auth URL without starting a redirect server. The token exchange never completes -- the "success" status is set prematurely.

**Solution:** Migrate `handleConnectGoogle` to use `startOAuthFlow()` (the redirect server flow used by ChatView). This requires:

1. Import `startOAuthFlow` and `cancelOAuthFlow` instead of `startOAuth`
2. Call `startOAuthFlow(engagementId, clientId, port, scopes)` -- requires active engagement
3. Subscribe to `oauth:token-stored` event before opening browser
4. Set `oauthStatus = "success"` only after event fires (or "error" on timeout)
5. Add 5-minute timeout with cancel cleanup (same pattern as ChatView)
6. Remove dead `startOAuth` import and the old `start_oauth` Rust command if no other consumers exist

## 4. Binary Path Resolution (Codex G1 + G2 -- the hard problem)

### 4.1 Problem

Under macOS App Sandbox, the `PATH` environment variable is restricted. Two critical binaries are affected:

1. **`claude` CLI** -- `session_manager.rs` uses `Command::new("claude")`. Under sandbox, this resolves to nothing.
2. **`npx`** -- `mcp_config.rs` hardcodes `"command": "npx"` for all 4 MCP servers. Under sandbox, `npx` is not on PATH.

Even with absolute paths, the sandbox may block execution of binaries outside the app bundle. The entitlement `com.apple.security.cs.allow-unsigned-executable-memory` may be needed for Node.js (V8 JIT).

### 4.2 Solution: Runtime Binary Resolver

New module: `src-tauri/src/claude/binary_resolver.rs`

```rust
pub struct ResolvedBinaries {
    pub claude: PathBuf,
    pub npx: PathBuf,
    pub node: PathBuf, // needed if MCP servers require node directly
}
```

**Resolution strategy (ordered by priority):**

For `claude`:
1. `which claude` (captures user's current PATH)
2. `~/.claude/local/bin/claude`
3. `/usr/local/bin/claude`
4. `/opt/homebrew/bin/claude`

For `npx`:
1. `which npx` (captures user's current PATH)
2. `/usr/local/bin/npx`
3. `/opt/homebrew/bin/npx`
4. `~/.nvm/versions/node/*/bin/npx` (glob for nvm users)
5. `~/.volta/bin/npx` (volta users)

For `node`:
1. Same pattern as `npx` but for `node` binary

**When resolution runs:**
- At app startup (in `lib.rs` setup hook), before sandbox restrictions apply
- Results cached in `{app_data_dir}/resolved-binaries.json`
- Re-resolved on each launch (paths may change if user updates tools)

**Consumers:**
- `session_manager.rs`: `Command::new(&resolved.claude)` instead of `Command::new("claude")`
- `mcp_config.rs`: `command: resolved.npx.to_string_lossy()` instead of `"npx"`
- Both receive `ResolvedBinaries` from app state (managed via `app.manage()`)

**Failure handling:**
- If `claude` not found: `setError("Claude CLI not found. Expected at: ~/.claude/local/bin/claude, /usr/local/bin/claude. Please install Claude Code.")`
- If `npx` not found: MCP servers are omitted from config (Claude still works, just without Gmail/Calendar/Drive). Warning shown in MCP health indicators.

### 4.3 Sandbox Subprocess Chain

The execution chain is: App (sandboxed) -> `claude` CLI -> reads `.mcp-config.json` -> spawns `npx` -> spawns MCP server.

Key insight: the `claude` CLI is spawned by our app (sandboxed), so it inherits our sandbox. But Claude CLI itself is not sandboxed -- it's an external binary. The sandbox restricts **our process's** ability to execute binaries, but once Claude CLI is running, it operates under its own permissions.

Therefore:
- Our app needs entitlements to **execute** the `claude` binary (absolute path)
- Claude CLI spawns `npx` on its own -- this is outside our sandbox boundary
- The `.mcp-config.json` must use absolute `npx` path because Claude CLI inherits the restricted `PATH` from our process

Testing required: verify that Claude CLI can spawn npx/MCP servers when launched from a sandboxed parent process with restricted PATH. If not, we may need to inject the full PATH into Claude CLI's environment.

## 5. Sandbox Entitlements + Capabilities

### 5.1 Entitlements (`src-tauri/entitlements.plist`)

New file for macOS App Sandbox + Hardened Runtime:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <!-- App Sandbox -->
    <key>com.apple.security.app-sandbox</key>
    <true/>

    <!-- Network: outbound HTTPS (googleapis, firebase, anthropic) -->
    <key>com.apple.security.network.client</key>
    <true/>

    <!-- Network: inbound localhost (OAuth redirect server) -->
    <key>com.apple.security.network.server</key>
    <true/>

    <!-- File access: user-selected workspace folder -->
    <key>com.apple.security.files.user-selected.read-write</key>
    <true/>

    <!-- Security-Scoped Bookmarks: persist workspace access -->
    <key>com.apple.security.files.bookmarks.app-scope</key>
    <true/>

    <!-- Hardened Runtime: allow Node.js V8 JIT (needed for npx/MCP servers) -->
    <key>com.apple.security.cs.allow-unsigned-executable-memory</key>
    <true/>
</dict>
</plist>
```

Notes:
- `network.server` is required for the OAuth redirect TCP listener on localhost
- `allow-unsigned-executable-memory` may be needed if Claude CLI spawns Node.js processes that use JIT. Must test empirically -- if not needed, remove (tighter sandbox).
- `com.apple.security.cs.allow-jit` is an alternative to `allow-unsigned-executable-memory` (more restrictive). Test which one Claude CLI / Node.js requires.

### 5.2 Restricted Capabilities (`src-tauri/capabilities/default.json`)

Replace broad `:default` scopes with specific permissions:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "opener:default",
    "shell:allow-execute",
    "fs:allow-read",
    "fs:allow-write",
    "sql:default",
    "http:allow-fetch",
    "notification:default",
    "dialog:default",
    "keyring:default"
  ]
}
```

Specific scoping TBD during implementation:
- `shell:allow-execute` -- ideally scope to only the resolved `claude` binary path. Tauri 2's shell plugin supports `scope` configuration in `capabilities/` for allowed commands. This must be configured to only allow executing `claude`.
- `fs:allow-read` / `fs:allow-write` -- scope to `$APPDATA/**` and the workspace root (resolved via Security-Scoped Bookmark). Tauri 2 supports path scope patterns.
- `http:allow-fetch` -- scope to `https://*.googleapis.com`, `https://*.firebaseio.com`, `https://*.firebaseapp.com`, `https://oauth2.googleapis.com`, `https://accounts.google.com`.

### 5.3 CSP Update

Add `http://localhost:*` to `connect-src` in `tauri.conf.json` for the OAuth redirect server:

```
connect-src ipc: http://ipc.localhost http://localhost:* https://*.googleapis.com https://*.firebaseio.com https://*.firebaseapp.com;
```

## 6. Security-Scoped Bookmarks

### 6.1 Problem

Under App Sandbox, when the user selects a workspace folder via the native file picker, the app gets temporary access. This access is revoked on app restart. Engagement vault paths (`~/.ikrs-workspace/vaults/{slug}/`) become inaccessible.

### 6.2 Solution

**Option A: `tauri-plugin-persisted-scope`** -- Tauri's official plugin that automatically creates and resolves Security-Scoped Bookmarks for paths accessed via `dialog:open`. If this plugin handles our use case (persist folder access selected via dialog), use it.

**Option B: Manual bookmark management** -- Use `objc2` crate to call `bookmarkData(options:includingResourceValuesForKeys:relativeTo:)` and `URLByResolvingBookmarkData`. Store bookmark data in app data dir.

**Preference:** Option A if it works. Investigate during implementation. If `tauri-plugin-persisted-scope` covers folder picker persistence, add it to `Cargo.toml` and wire up in `lib.rs`. If not, implement Option B.

### 6.3 Bookmark Lifecycle

1. **First launch / new engagement:** User selects workspace root via `dialog:open`. Plugin creates Security-Scoped Bookmark automatically.
2. **Subsequent launches:** Plugin resolves stored bookmark, calls `startAccessingSecurityScopedResource()`. Workspace folder is accessible.
3. **Bookmark revocation** (rare, after OS update): Handle `EPERM` on workspace access by showing "Please re-select your workspace folder" dialog. Clear stale bookmark.

## 7. Cross-Platform Compilation Guards (Codex A4)

`registry.rs` uses Unix-only commands (`ps`, `kill`). For Approach B (unsigned Windows builds), these must compile on Windows.

**Solution:** `#[cfg(target_family = "unix")]` guards with Windows no-op stubs.

```rust
#[cfg(target_family = "unix")]
fn is_process_alive(pid: u32) -> bool {
    // existing ps -p implementation
}

#[cfg(target_family = "windows")]
fn is_process_alive(_pid: u32) -> bool {
    false // no-op for now, Windows orphan cleanup deferred
}
```

Same pattern for `is_claude_process()` and `kill_process()`. The `cleanup_orphans()` function remains cross-platform -- it just does nothing meaningful on Windows. Full Windows implementation deferred to a future phase.

## 8. macOS Bundle Configuration

Add macOS-specific config to `tauri.conf.json`:

```json
{
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": ["...existing..."],
    "macOS": {
      "minimumSystemVersion": "12.0",
      "entitlements": "entitlements.plist",
      "signingIdentity": "-",
      "frameworks": []
    },
    "category": "public.app-category.business"
  }
}
```

Notes:
- `minimumSystemVersion: "12.0"` -- macOS Monterey+ (Codex I4)
- `signingIdentity: "-"` -- placeholder for development builds; CI overrides with real identity via env var
- `entitlements` points to the plist created in Section 5.1
- `category: "public.app-category.business"` -- appropriate for a professional services workspace tool

## 9. CI Signing + Notarization

### 9.1 GitHub Actions Secrets (human setup, after Apple Developer enrollment)

| Secret | Value |
|--------|-------|
| `APPLE_CERTIFICATE` | Base64-encoded .p12 Developer ID Application certificate |
| `APPLE_CERTIFICATE_PASSWORD` | .p12 export password |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: IKAROS FZ-LLC (TEAM_ID)` |
| `APPLE_ID` | Apple ID email used for notarization |
| `APPLE_PASSWORD` | App-specific password (generated at appleid.apple.com) |
| `APPLE_TEAM_ID` | 10-character Team ID from Apple Developer portal |

### 9.2 CI Workflow Changes

The macOS build step in `.github/workflows/ci.yml` already uses `tauri-apps/tauri-action@v0`. This action supports code signing and notarization via environment variables:

```yaml
- uses: tauri-apps/tauri-action@v0
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
    # macOS signing
    APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
    APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
    APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
    # macOS notarization
    APPLE_ID: ${{ secrets.APPLE_ID }}
    APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
    APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
```

When secrets are not set (Linux/Windows runners, or before Apple account is ready), `tauri-action` skips signing gracefully. No conditional logic needed.

### 9.3 Release Artifact Upload

Add artifact upload step after build:
```yaml
- name: Upload artifacts
  uses: actions/upload-artifact@v4
  with:
    name: ikrs-workspace-${{ matrix.target }}
    path: src-tauri/target/release/bundle/**
```

This produces downloadable `.dmg` (macOS), `.deb` + `.AppImage` (Linux), `.msi` (Windows) from CI.

## 10. Risk Register

| ID | Risk | Severity | Mitigation |
|----|------|----------|------------|
| P4-R1 | Claude CLI not found under sandbox PATH | HIGH | Binary resolver with fallback paths + diagnostic UI |
| P4-R2 | npx/MCP subprocess chain blocked by sandbox | HIGH | Absolute paths in .mcp-config.json + PATH env injection into Claude CLI |
| P4-R3 | Security-Scoped Bookmark revoked after OS update | MEDIUM | Handle EPERM, prompt re-selection of workspace folder |
| P4-R4 | Apple rejects app due to shell:execute | MEDIUM | Scope shell permission to only `claude` binary |
| P4-R5 | OAuth redirect fails under sandbox network | LOW | `network.server` entitlement for localhost binding |
| P4-R6 | Notarization fails due to unsigned dylibs | MEDIUM | Verify all bundled native deps are signed by tauri-action |
| P4-R7 | Apple Developer enrollment delayed (24-48h) | LOW | All non-signing work proceeds independently |
| P4-R8 | V8 JIT blocked by Hardened Runtime | MEDIUM | Test with/without `allow-unsigned-executable-memory`; fallback to `allow-jit` |

## 11. Task Waves

| Wave | Tasks | Depends On |
|------|-------|-----------|
| **1: Phase 3 debt** | P1 (identifier + migration), P2 (refresh_token refactor), P3 (SettingsView OAuth) | Push 35 unpushed commits first |
| **2: Sandbox prep** | Binary path resolver, restricted capabilities, entitlements.plist, Security-Scoped Bookmarks, Windows cfg guards | Wave 1 |
| **3: Build config** | macOS bundle config in tauri.conf.json, CSP update | Wave 2 |
| **4: CI signing** | Code signing + notarization env vars in workflow, artifact upload | Wave 3 + Apple Developer account |
| **5: Validation** | End-to-end test: install .dmg on clean macOS, create engagement, connect Google, start Claude session, verify MCP servers | Wave 4 |

## 12. Exit Criteria

- [ ] Signed .dmg installs on macOS 12+ without Gatekeeper warning
- [ ] Claude CLI spawns correctly from sandboxed app (absolute path)
- [ ] MCP servers (Gmail, Calendar, Drive, Obsidian) connect via absolute npx path
- [ ] OAuth flow completes end-to-end (SettingsView and ChatView)
- [ ] Tokens auto-refresh (no re-auth within 1 hour)
- [ ] Workspace folder access persists across app restart (Security-Scoped Bookmarks)
- [ ] Unsigned Linux (.deb, .AppImage) and Windows (.msi) build in CI
- [ ] 100+ tests passing (existing 100 + new tests for resolver, token refresh, migration)

## 13. Blocking Human Task

**Apple Developer Program enrollment** ($99/year) must be completed before Wave 4 (CI signing). All other waves proceed independently.

Steps:
1. Enroll at developer.apple.com/programs
2. Create Developer ID Application certificate
3. Create Developer ID Installer certificate
4. Generate app-specific password at appleid.apple.com
5. Add 6 secrets to GitHub repository settings
