# Phase 4b: Distribution Polish — Offline, Auto-Update, DMG

**Status:** Implementation Complete (pending Apple Developer enrollment + updater keypair generation)
**Parent spec:** `embedded-claude-architecture.md` (Phase 4: Distribution)
**Predecessor:** `m2-phase4a-sandbox-signing-design.md` (PASS 8.5/10, all tasks complete)
**Codex reviews:** Scope review PASS (2026-04-13), Spec WARN 7.5/10 → PASS 8.5/10 (re-review, all conditions resolved)

## 1. Goal

Complete all remaining distribution work so the app is fully shippable:
- Graceful offline behavior matching parent spec Section 3.15
- Auto-update so users never manually download a new version
- Professional DMG installer with IKAROS branding

After Phase 4b, the app is production-ready for distribution to IKAROS consultants.

## 2. Out of Scope

- Windows code signing (future)
- App Store submission (future)
- Background update polling (manual check + on-launch only)
- "What's new" changelog display after update (future)

## 3. Offline Detection + Graceful Degradation (P1)

### 3.1 Current State

- `src/hooks/useOnlineStatus.ts` exists and is actively used in `App.tsx` → `StatusBar` (shows "Offline" indicator). The hook is wired but no view-level offline guards exist.
- ChatView shows errors via `error` state but has no offline-specific messages
- SettingsView has no offline guard on the OAuth button
- `useWorkspaceSession.connect()` calls `spawnClaudeSession` without checking connectivity
- Parent spec (Section 3.15) mandates specific error messages:
  - Spawn failure: "Unable to reach Claude. Check your internet connection and try again."
  - Mid-session loss: "Connection interrupted. Your work is saved locally."

### 3.2 Offline Banner Component

New file: `src/components/OfflineBanner.tsx`

Reusable banner shown at the top of views that require internet:

```tsx
interface OfflineBannerProps {
  feature: string; // e.g. "Claude", "Gmail", "Google Calendar"
}
```

Renders: `"You're offline. {feature} requires an internet connection."` in a warning-colored bar. Shown only when `useOnlineStatus()` returns `false`.

**Parent spec deviation (Codex I1):** The parent spec (Section 3.15) uses a generic message "Google services unavailable offline." for MCP services. This spec uses per-service messages (e.g., "Gmail requires an internet connection.") for better UX. The parent spec Section 3.15 should be amended to match these improved messages.

### 3.3 View Integration

| View | Requires Internet? | Offline Behavior |
|------|-------------------|------------------|
| ChatView | Yes (Claude session) | Show OfflineBanner. Disable "Connect to Claude" button. If mid-session: augment error with "Connection interrupted" message. |
| InboxView | Yes (Gmail MCP) | Show OfflineBanner: "Gmail requires an internet connection." |
| CalendarView | Yes (Calendar MCP) | Show OfflineBanner: "Google Calendar requires an internet connection." |
| SettingsView | Partial (OAuth only) | Disable "Connect Google Account" button with tooltip "Sign in requires internet." |
| FilesView | Yes (Google Drive MCP) | Show OfflineBanner: "Google Drive requires an internet connection." (**Parent spec deviation:** Section 3.15 lists file browsing as offline-functional, but our implementation uses Google Drive MCP which requires internet. Parent spec should be amended.) |
| TasksView | No | Fully functional offline (local SQLite) |
| NotesView | Partial (MCP via Claude) | Notes require an active Claude session. When offline, vault files are inaccessible. Show OfflineBanner if no active session. (**Parent spec deviation:** Section 3.15 lists notes as offline-functional, but our implementation uses Obsidian MCP via Claude session. Parent spec should be amended.) |

### 3.4 Connect Guard

In `useWorkspaceSession.connect()` **and** `useWorkspaceSession.switchEngagement()`, add an online check before the preflight/spawn:

```typescript
if (!navigator.onLine) {
  useClaudeStore.getState().setError(
    "Unable to reach Claude. Check your internet connection and try again."
  );
  return;
}
```

Both functions spawn Claude sessions — `connect()` for initial connection and `switchEngagement()` when switching between engagements. The guard must be applied to both entry points to prevent spawn attempts while offline.

This uses the exact wording from the parent spec Section 3.15.

### 3.5 Mid-Session Loss Detection

When a `claude:error` or `claude:session-crashed` event fires while `!navigator.onLine`, the ChatView error display should show:

"Connection interrupted. Your work is saved locally."

instead of the generic error. The retry button remains.

**Implementation:** In ChatView's error rendering, check `navigator.onLine` at render time. If `!isOnline && error`, display the connectivity message. If `isOnline && error`, display the original error (API-side issue, not local connectivity). No debounce needed — `navigator.onLine` reflects current state at render time, and the retry button handles transient blips regardless of which message is shown.

### 3.6 Test Plan

- Unit test for `useOnlineStatus` hook (mock `navigator.onLine` + dispatch events)
- Unit test for `OfflineBanner` component (renders when offline, hidden when online)
- Unit test for `connect()` guard (returns early with correct error when offline)

## 4. Auto-Update via tauri-plugin-updater (P2)

### 4.1 Architecture

Tauri's updater plugin checks a remote JSON manifest for newer versions. When found, it downloads the update, verifies the signature, and replaces the app binary. On macOS, this replaces the `.app` bundle; on Linux, the AppImage; on Windows, the NSIS installer.

**Update flow:**
1. On app launch: check for updates silently
2. In Settings: "Check for Updates" button for manual check
3. If update available: show notification with version + "Install & Restart"
4. User clicks install: download with progress → replace → restart

**Update endpoint:** GitHub Releases. `tauri-action` generates `latest.json` manifest automatically when creating a release.

**Update signing:** Tauri updater requires its own keypair (separate from Apple code signing). Generate with `tauri signer generate`. Private key → GitHub secret. Public key → `tauri.conf.json`.

### 4.2 Plugin Setup

**Rust side:**
- Add `tauri-plugin-updater = "2"` to `src-tauri/Cargo.toml`
- Register `.plugin(tauri_plugin_updater::Builder::new().build())` in `src-tauri/src/lib.rs`

**JS side:**
- Add `@tauri-apps/plugin-updater` to `package.json`
- Add `"updater:default"` to `src-tauri/capabilities/default.json`

**Config in `tauri.conf.json`:**
```json
{
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/IKAROSgit/ikrs-workspace/releases/latest/download/latest.json"
      ],
      "pubkey": "GENERATED_PUBLIC_KEY_HERE"
    }
  }
}
```

Note: If the GitHub repo is private, the endpoint must be changed to a publicly accessible URL (e.g., GitHub Pages or a static CDN). The current repo visibility determines this.

### 4.3 Version Display + Update UI

In `SettingsView.tsx`, add a section at the bottom:

```
App Version: {version}
[Check for Updates]
```

The version string is obtained at runtime via `getVersion()` from `@tauri-apps/api/app` — never hardcoded. This ensures the displayed version always matches the actual binary version from `tauri.conf.json`.

When an update is found:
```
Update Available: v0.2.0
[Install & Restart]
```

During download, show a progress bar or spinner.

New component: `src/components/UpdateChecker.tsx` — encapsulates both version display (via `getVersion()`) and update check/install logic (via `@tauri-apps/plugin-updater`'s `check()` and `downloadAndInstall()` APIs). If the update check fails (e.g., offline or network error), the version display remains and the error is silently ignored — updates are non-critical and the user can retry manually.

### 4.4 CI Release Workflow

The current CI workflow (`.github/workflows/ci.yml`) builds on every push/PR but does not create GitHub Releases. For the updater to work, a release workflow is needed.

**Approach:** Extend the existing CI workflow with a release job that triggers on tags:

```yaml
on:
  push:
    branches: [main]
    tags:
      - 'v*'
  pull_request:
    branches: [main]
```

The `tauri-action` step gains additional parameters when triggered by a tag:
- `tagName: ${{ github.ref_name }}`
- `releaseName: "IKAROS Workspace ${{ github.ref_name }}"`
- `releaseBody: "See changelog for details."`

The `tauri-action` step also requires updater signing env vars (in addition to the existing Apple signing vars from Phase 4a):
```yaml
env:
  TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
```

This produces a GitHub Release with the `.dmg`, `.deb`, `.AppImage`, `.msi`, and `latest.json` manifest.

### 4.5 Signing Keypair (Human Task)

Generate on the development machine:
```bash
npx tauri signer generate -w ~/.tauri/ikrs-workspace.key
```

This produces:
- Private key: `~/.tauri/ikrs-workspace.key` → GitHub secret `TAURI_SIGNING_PRIVATE_KEY`
- Private key password → GitHub secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- Public key → paste into `tauri.conf.json` `plugins.updater.pubkey`

### 4.6 Test Plan

- Unit test for `UpdateChecker` component (renders version, handles check/install states)
- Integration: tag a test release, verify `latest.json` is generated, verify update check finds it

## 5. DMG Visual Polish (P3)

### 5.1 DMG Background Image

Create `src-tauri/icons/dmg-background.png` (660x400 @2x = 1320x800 actual):
- IKAROS logo/branding in the background
- Visual arrow or cue pointing from the app icon to the Applications folder
- Professional, minimal design matching IKAROS brand (DM Sans typography, brand colors)

Placeholder: A solid branded background with text "Drag to Applications" is acceptable until the final design asset is provided.

### 5.2 DMG Configuration

In `tauri.conf.json`, add to `bundle.macOS`:

```json
"dmg": {
  "background": "icons/dmg-background.png",
  "windowSize": { "width": 660, "height": 400 },
  "appPosition": { "x": 180, "y": 170 },
  "applicationFolderPosition": { "x": 480, "y": 170 }
}
```

### 5.3 Test Plan

- Visual verification: build DMG locally (or from CI), mount, verify background, icon positions, and drag-to-Applications flow

## 6. Risk Register

| ID | Risk | Severity | Mitigation |
|----|------|----------|------------|
| P4b-R1 | `navigator.onLine` unreliable on captive portals | LOW | Acceptable for MVP; HTTP probe enhancement deferred |
| P4b-R2 | Private GitHub repo blocks update endpoint | MEDIUM | Check repo visibility; if private, use GitHub Pages or public CDN |
| P4b-R3 | macOS privilege escalation for `/Applications` update | LOW | Tauri handles via NSAppleScript elevation |
| P4b-R4 | DMG background image not provided by designer | LOW | Use text-only placeholder; replace when asset ready |
| P4b-R5 | Update signing keypair management | MEDIUM | Document generation steps; store private key only in GitHub secrets |

## 7. Task Waves

| Wave | Tasks | Depends On |
|------|-------|-----------|
| **1: Offline** | OfflineBanner component, wire into ChatView/InboxView/CalendarView/FilesView/NotesView/SettingsView, connect + switchEngagement guards, mid-session loss detection, tests | Nothing |
| **2: Auto-update** | Updater plugin setup, tauri.conf.json config, UpdateChecker component, version display in Settings, release workflow in CI | Wave 1 (offline-aware update check) |
| **3: DMG polish** | Background image placeholder, DMG config in tauri.conf.json | Wave 2 (release workflow must exist) |
| **4: Validation** | Full test suite, build verification, spec update | Wave 3 |

## 8. Exit Criteria

- [ ] OfflineBanner appears in ChatView, InboxView, CalendarView, FilesView when offline
- [ ] NotesView shows OfflineBanner when no active Claude session (MCP requires session)
- [ ] "Connect to Claude" disabled when offline with spec-mandated message
- [ ] `switchEngagement()` guarded with same offline check as `connect()`
- [ ] Mid-session loss shows "Connection interrupted. Your work is saved locally." + retry
- [ ] OAuth button disabled when offline with "Sign in requires internet."
- [ ] TasksView remains fully functional offline (local SQLite)
- [ ] `tauri-plugin-updater` integrated with signing keypair
- [ ] CI produces GitHub Releases with update manifests on tag push
- [ ] SettingsView shows current version (via `getVersion()`) + "Check for Updates" button
- [ ] Update notification with "Install & Restart" action
- [ ] DMG background image with IKAROS branding and drag-to-Applications cue
- [ ] DMG window size and icon positions configured
- [ ] All existing tests (113) continue to pass
- [ ] New tests for offline detection and update checker
