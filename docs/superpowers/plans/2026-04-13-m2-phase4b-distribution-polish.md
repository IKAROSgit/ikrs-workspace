# Phase 4b: Distribution Polish — Offline, Auto-Update, DMG (Retroactive Plan)

> **Status:** Complete (retroactive) — code shipped 2026-04-13 under commit `b4bb80a`; this plan was written 2026-04-17 to close the Golden Rule #10/#12 planning-trail gap flagged by `.output/codex-reviews/2026-04-17-m2-phase4b-retroactive-signoff.md` (C5).
>
> **For agentic workers:** This plan is *descriptive*, not *prescriptive*. The implementation has already landed. Steps are checkbox (`- [x]`) to reflect shipped state. Use this file as the audit trail that the work happened, and as the breadcrumb for the known follow-up items listed in "Known Carry-Over Work" below.

**Phase:** M2 / Phase 4b (Distribution Polish)
**Spec:** `docs/specs/m2-phase4b-distribution-polish-design.md` (270 lines)
**Predecessor plan:** `docs/superpowers/plans/2026-04-12-m2-phase4a-sandbox-signing.md`
**Date range:** 2026-04-13 (single-day execution — squash-merged to `main`)
**Shipped commit:** `b4bb80a  feat: Phase 4b — offline detection, auto-update, DMG polish`
**Parent commit:** `01ef2a6` (Phase 4a wrap-up)
**Codex pre-merge sign-off:** NONE on record (Golden Rule #11 gap)
**Codex retroactive sign-off:** `.output/codex-reviews/2026-04-17-m2-phase4b-retroactive-signoff.md` — **APPROVED WITH CONDITIONS, 7.0/10** (Security FAIL, Readiness FAIL; conditions C1–C7 open)

---

## Author Note (Retroactive Disclosure)

Phase 4b was implemented directly from the design spec without an executable plan file being committed under `docs/superpowers/plans/`. The last plan on record at the time was `2026-04-12-m2-phase4a-sandbox-signing.md`. The proxy review (`.output/codex-reviews/2026-04-16-m2-phase4b-final-review.md`) and this retroactive plan are both post-hoc reconstructions from the diff of `b4bb80a`. Do **not** treat this document as evidence of pre-merge planning rigor. It is a backfill intended to:

1. Make the wave/task structure legible for Phase 5 planners who need to reference what actually shipped.
2. Surface the 7 carry-over conditions (C1–C7) from the retroactive Codex review so they do not get lost.
3. Satisfy Golden Rule #10 (every phase has a plan) and #12 (documentation-of-record exists for every phase) in arrears.

Any deviations between this plan's task descriptions and the shipped code are noted inline as "Spec/diff deviation" callouts.

---

## Goal (paraphrased from spec §1)

Complete all remaining distribution work so the app is shippable to IKAROS consultants: (a) graceful offline behaviour matching parent-spec §3.15 across every internet-dependent view, (b) auto-update via `tauri-plugin-updater` wired through Settings with `getVersion()` + `check()` + `downloadAndInstall()`, and (c) a professionally-configured macOS `.dmg` installer with IKAROS branding and drag-to-Applications layout. After Phase 4b, the only remaining distribution blockers are human/operational: Apple Developer enrollment (inherited from Phase 4a) and generation of the updater signing keypair (spec §4.5).

## Out of Scope (per spec §2)

- Windows code signing
- App Store submission
- Background update polling (manual check + silent on-launch only)
- Post-update "what's new" changelog surface

---

## File Map (from diff of `b4bb80a`)

| Action | File | Responsibility | Wave |
|--------|------|---------------|------|
| Create | `src/components/OfflineBanner.tsx` | Reusable banner; reads `useOnlineStatus()`; hides when online | 1 |
| Modify | `src/hooks/useWorkspaceSession.ts` | Offline guards in `connect()` + `switchEngagement()` | 1 |
| Modify | `src/views/ChatView.tsx` | Wire OfflineBanner (both branches); disable Connect button offline; mid-session loss text | 1 |
| Modify | `src/views/InboxView.tsx` | Wire OfflineBanner ("Gmail") | 1 |
| Modify | `src/views/CalendarView.tsx` | Wire OfflineBanner ("Google Calendar") | 1 |
| Modify | `src/views/FilesView.tsx` | Wire OfflineBanner ("Google Drive") | 1 |
| Modify | `src/views/NotesView.tsx` | Wire OfflineBanner ("Notes (Obsidian MCP)") + "requires active Claude session" inline notice | 1 |
| Modify | `src/views/SettingsView.tsx` | Disable OAuth button offline with tooltip; mount `<UpdateChecker />` in About card | 1 + 2 |
| Create | `tests/unit/hooks/useOnlineStatus.test.ts` | 5 tests for the pre-existing hook (coverage backfill) | 1 |
| Create | `tests/unit/components/OfflineBanner.test.tsx` | 4 tests (hidden online, shown offline, feature interpolation, aria) | 1 |
| Create | `tests/unit/hooks/useWorkspaceSession-offline.test.ts` | 4 tests for `connect()`/`switchEngagement()` offline guards | 1 |
| Modify | `src-tauri/Cargo.toml` | Add `tauri-plugin-updater = "2"` | 2 |
| Modify | `src-tauri/Cargo.lock` | Updater plugin dependency closure (+178 lines) | 2 |
| Modify | `src-tauri/src/lib.rs` | `.plugin(tauri_plugin_updater::Builder::new().build())` | 2 |
| Modify | `src-tauri/capabilities/default.json` | Add `"updater:default"` to main window permissions | 2 |
| Modify | `src-tauri/tauri.conf.json` | Add `plugins.updater` block (endpoint + pubkey) and `bundle.macOS.dmg` block | 2 + 3 |
| Modify | `package.json` | Add `@tauri-apps/plugin-updater: ^2.10.1` | 2 |
| Modify | `package-lock.json` | JS updater plugin closure (+10 lines) | 2 |
| Create | `src/components/UpdateChecker.tsx` | 96 LOC — version display + check/install state machine | 2 |
| Create | `tests/unit/components/UpdateChecker.test.tsx` | 4 tests (idle render, check flow, install flow, error path) | 2 |
| Modify | `.github/workflows/ci.yml` | Tag trigger + `tauri-action` `with:` block + `TAURI_SIGNING_PRIVATE_KEY*` env vars | 2 |
| Create | `src-tauri/icons/dmg-background.png` | 1689-byte placeholder (660×400, IKAROS brand colour) | 3 |
| Create | `docs/specs/m2-phase4b-distribution-polish-design.md` | Design spec itself committed alongside the work (+270 lines) | 0 |

**Total:** 23 files changed, +1006 / −10. Tests: 129 total at HEAD (58 Rust + 71 JS; +16 net new JS tests from Phase 4b).

---

## Wave 1: Offline Detection + Graceful Degradation (P1)

Spec refs: §3.1 – §3.6. Commit: `b4bb80a` (single-squash).

### Task 1.1: OfflineBanner component

**Files:** Create `src/components/OfflineBanner.tsx` (17 LOC)

- [x] Component accepts a single prop `feature: string` and renders `"You're offline. {feature} requires an internet connection."` inside an amber-500/10 warning bar with an amber-500/20 bottom border.
- [x] Returns `null` when `useOnlineStatus()` is `true` (pure presentation; the hook already existed pre-4b).
- [x] Tailwind classes: `bg-amber-500/10 text-amber-700 dark:text-amber-400 text-sm border-b border-amber-500/20`, padded `px-4 py-2`, flex row with `gap-2`.

**Spec/diff alignment:** Byte-for-byte match with spec §3.2.

### Task 1.2: Connect + switchEngagement offline guards

**Files:** Modify `src/hooks/useWorkspaceSession.ts` (two insertions, +15 lines)

- [x] In `connect()` (new lines 41–46): `if (!navigator.onLine)` → `useClaudeStore.getState().setError("Unable to reach Claude. Check your internet connection and try again.")` → early `return`.
- [x] In `switchEngagement()` (new lines 104–109): identical guard, placed *after* the `if (switching) return` short-circuit but *before* `setSwitching(true)` so offline transitions do not flip the switching flag.
- [x] Error text matches parent-spec §3.15 verbatim.

**Spec/diff alignment:** Exact. Codex retroactive review §6 confirms "Exact" match on both guard sites.

### Task 1.3: Wire OfflineBanner into views

**Files:** Modify 5 view files

- [x] `src/views/ChatView.tsx` — two insertions (disconnected branch at line ~120 wraps the existing content in a `<div className="flex flex-col h-full">` with `<OfflineBanner feature="Claude" />` above it; connected branch at line ~134 prepends the banner). Connect button gets `disabled={!isOnline}` + `title={!isOnline ? "Requires internet connection." : undefined}`. **Note:** `useOnlineStatus()` is imported here for the disabled flag and for mid-session text (Task 1.4). (Codex §2 WARN: double-banner render — only one branch is live at a time, cosmetic refactor target, not a bug.)
- [x] `src/views/InboxView.tsx` — single `<OfflineBanner feature="Gmail" />` above the existing top bar.
- [x] `src/views/CalendarView.tsx` — `<OfflineBanner feature="Google Calendar" />`.
- [x] `src/views/FilesView.tsx` — `<OfflineBanner feature="Google Drive" />`.
- [x] `src/views/NotesView.tsx` — `<OfflineBanner feature="Notes (Obsidian MCP)" />` **plus** an inline notice rendered when `claudeStatus !== "connected"`: "Notes require an active Claude session. Connect to Claude to access vault files." in a `bg-muted text-muted-foreground` bar.

**Spec/diff deviation:** Spec §3.3 for NotesView reads "Notes require an active Claude session. When offline, vault files are inaccessible." — the shipped inline notice reads "Notes require an active Claude session. Connect to Claude to access vault files." and is gated on session status, not on `navigator.onLine`. The offline case is covered by the banner; the inline notice handles the orthogonal "online but disconnected from Claude" case. Net effect: stricter than the spec, spec §3.3 could be amended to match.

### Task 1.4: Mid-session connection-loss override

**Files:** Modify `src/views/ChatView.tsx` (lines ~149–155)

- [x] Import `useOnlineStatus`; derive `isOnline` at the top of the component.
- [x] In the error-render block, replace `<span>{error}</span>` with a ternary:
  ```tsx
  <span>
    {!isOnline
      ? "Connection interrupted. Your work is saved locally."
      : error}
  </span>
  ```
- [x] Retry button is preserved unchanged on the right of the error bar.

**Spec/diff alignment:** Exact (§3.5). Codex §6 confirms.

### Task 1.5: SettingsView OAuth offline guard

**Files:** Modify `src/views/SettingsView.tsx`

- [x] Import `useOnlineStatus`; derive `isOnline`.
- [x] "Connect Google Account" button: `disabled={oauthStatus === "pending" || !isOnline}` with `title={!isOnline ? "Sign in requires internet." : undefined}`.

**Spec/diff alignment:** Exact (§3.3 SettingsView row).

### Task 1.6: Wave 1 tests

**Files:** Create three new test files (+276 LOC)

- [x] `tests/unit/hooks/useOnlineStatus.test.ts` — 5 tests (95 LOC). Covers mount state, `online`/`offline` event dispatch, cleanup, SSR safety.
- [x] `tests/unit/components/OfflineBanner.test.tsx` — 4 tests (69 LOC). Covers hidden-when-online, shown-when-offline, feature-name interpolation, role/aria.
- [x] `tests/unit/hooks/useWorkspaceSession-offline.test.ts` — 4 tests (112 LOC). Covers `connect()` offline guard sets error + early returns, `switchEngagement()` offline guard, online passthrough for both.

**Result at HEAD:** 71 JS tests passing (commit body claim). Wave 1 contributes 13 of the +16 net-new JS tests.

---

## Wave 2: Auto-Update via `tauri-plugin-updater` (P2)

Spec refs: §4.1 – §4.6. Same squash commit `b4bb80a`.

### Task 2.1: Rust-side plugin wiring

**Files:** `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`

- [x] Add `tauri-plugin-updater = "2"` to `[dependencies]` in `Cargo.toml` (line 26, below `tauri-plugin-persisted-scope`).
- [x] Regenerate `Cargo.lock` (+178 lines — transitive update crate closure including `minisign-verify`, `tar`, `zip`).
- [x] Register in `src-tauri/src/lib.rs:54`: `.plugin(tauri_plugin_updater::Builder::new().build())`, placed after `tauri_plugin_keyring::init()` and before the `.manage()` calls.
- [x] Append `"updater:default"` to `src-tauri/capabilities/default.json` permissions array (last element).

**Spec/diff deviation (Codex §3):** Capability grant is coarse — Tauri v2 updater permissions are allow/deny only, no per-host scoping. Spec §4.2 does not call this out; Codex C4 recommends documenting the limitation in `.architecture/SECURITY.md`.

### Task 2.2: JS-side plugin wiring

**Files:** `package.json`, `package-lock.json`

- [x] Add `"@tauri-apps/plugin-updater": "^2.10.1"` to `dependencies`.
- [x] `npm install` regenerates `package-lock.json` (+10 lines).

### Task 2.3: `tauri.conf.json` updater config

**Files:** `src-tauri/tauri.conf.json`

- [x] Insert a `plugins.updater` block between `app.security` and `bundle`:
  ```json
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/IKAROSgit/ikrs-workspace/releases/latest/download/latest.json"
      ],
      "pubkey": "GENERATED_PUBLIC_KEY_HERE"
    }
  }
  ```

**Spec/diff deviation (CRITICAL — Codex §3 FAIL, blocks release):** The `"pubkey"` value shipped as the literal string `"GENERATED_PUBLIC_KEY_HERE"` rather than an actual base64-encoded ed25519 public key. Spec §4.5 explicitly carves this out as a human task to be completed on the developer machine. Shipping with the placeholder is fail-closed (Tauri's verifier rejects every signed manifest against an invalid anchor), so there is no RCE *today*, but the first `v*` tag cut will produce a release whose auto-updates cannot be verified. Also, endpoint reachability (P4b-R2) is unverified — if `IKAROSgit/ikrs-workspace` is private, this endpoint 404s. Both are tracked as Codex conditions C1 and C2.

### Task 2.4: `UpdateChecker` component

**Files:** Create `src/components/UpdateChecker.tsx` (96 LOC)

- [x] Local state: `version: string`, `status: "idle" | "checking" | "available" | "downloading" | "error"`, `update: Update | null`, `errorMsg: string`.
- [x] On mount: `getVersion().then(setVersion).catch(() => setVersion("unknown"))`.
- [x] Silent update check on mount (`checkForUpdates(true)`) — errors suppressed, state stays `idle`.
- [x] `checkForUpdates(silent)` — calls `check()` from `@tauri-apps/plugin-updater`; transitions `idle → checking → (available | idle | error)`.
- [x] `handleInstall()` — `update.downloadAndInstall()`; transitions `available → downloading → (restart | error)`.
- [x] Renders "App Version: {version}" always; conditional UI per state (update banner, progress text, error text, "Check for Updates" button in idle/error).

**Spec/diff alignment:** Exact match with spec §4.3. State machine is the minimal correct shape flagged as a strength in Codex §2.

### Task 2.5: Mount UpdateChecker in Settings

**Files:** `src/views/SettingsView.tsx`

- [x] Import `UpdateChecker`.
- [x] Append an "About" `<Card>` after the SkillStatusPanel block (post-Phase-3b), containing `<UpdateChecker />` in its `CardContent`.

### Task 2.6: CI release workflow extension

**Files:** `.github/workflows/ci.yml`

- [x] Add tag trigger to the `on.push` block:
  ```yaml
  tags:
    - 'v*'
  ```
- [x] Add `tauri-action` `with:` block that conditionally populates `tagName`, `releaseName`, `releaseBody` only when `github.ref_type == 'tag'` (ternary empty-string fallback for non-tag runs).
- [x] Add updater signing env vars alongside the Phase 4a Apple signing env vars:
  ```yaml
  TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
  ```

**Spec/diff deviation (Codex §3 — C3):** No hard-fail guard was added for missing `TAURI_SIGNING_PRIVATE_KEY` on tag push. Spec §4.4 does not mandate one but Codex flags its absence as a release-hardening gap. The recommended snippet is captured in C3 below.

### Task 2.7: UpdateChecker tests

**Files:** Create `tests/unit/components/UpdateChecker.test.tsx` (68 LOC, 4 tests)

- [x] Renders version placeholder, then resolved version.
- [x] Check-for-updates click transitions to `checking` and then `available` when `check()` resolves with an update.
- [x] Install click calls `downloadAndInstall()`.
- [x] Error path renders error message.

---

## Wave 3: DMG Visual Polish (P3)

Spec refs: §5.1 – §5.3. Same squash commit.

### Task 3.1: DMG background placeholder

**Files:** Create `src-tauri/icons/dmg-background.png` (1689 bytes, 660×400)

- [x] Solid IKAROS brand-colour background; text "Drag to Applications" per spec §5.1 placeholder allowance.

**Spec/diff deviation (Codex §4 — C6):** Final designer-delivered asset with logo and visual drag-arrow has not been produced. Placeholder is explicitly permitted by spec §5.1 and acknowledged in risk P4b-R4.

### Task 3.2: DMG configuration

**Files:** `src-tauri/tauri.conf.json` (inside `bundle.macOS`)

- [x] Add the `dmg` sub-block byte-for-byte per spec §5.2:
  ```json
  "dmg": {
    "background": "icons/dmg-background.png",
    "windowSize": { "width": 660, "height": 400 },
    "appPosition": { "x": 180, "y": 170 },
    "applicationFolderPosition": { "x": 480, "y": 170 }
  }
  ```

### Task 3.3: DMG validation

- [ ] **Not performed.** Spec §5.3 calls for a visual verification by building the DMG locally (or pulling from CI), mounting it, and confirming the icon positions and drag flow. No build artefact or screenshot is attached in `.output/`. Tracked as a Codex §4 completeness WARN.

---

## Wave 4: Validation (spec §7 wave 4)

- [x] **Rust tests:** 58 passing at HEAD (no Rust code added in Phase 4b, but the updater plugin must link and compile under the existing sandbox + entitlements config from Phase 4a).
- [x] **JS tests:** 71 passing at HEAD (+16 net new from Phase 4b).
- [x] **Aggregate:** 129 tests passing per commit body.
- [ ] **Spec update:** Spec status at `docs/specs/m2-phase4b-distribution-polish-design.md:3` reads "Implementation Complete (pending Apple Developer enrollment + updater keypair generation)" — accurate.
- [ ] **DMG end-to-end build:** Not performed in CI with real Apple credentials (Phase 4a left signing secrets as unset-skips-gracefully; Phase 4b did not change that posture). Codex §4 WARN.

---

## Codex Checkpoints

| Checkpoint | Planned per house style | Actual |
|------------|-------------------------|--------|
| Post-Wave 1 scope review | Yes | **NOT PERFORMED** — no pre-merge Codex review on record for `b4bb80a` |
| Post-Wave 2 security review | Yes (updater is highest-risk surface) | **NOT PERFORMED** |
| Pre-merge sign-off on `b4bb80a` | Yes (Golden Rule #11) | **NOT PERFORMED** |
| Proxy review (post-merge) | — | `.output/codex-reviews/2026-04-16-m2-phase4b-final-review.md` — APPROVED WITH CONDITIONS 7.0/10, acknowledged as after-the-fact |
| Retroactive 7-point sign-off | — | `.output/codex-reviews/2026-04-17-m2-phase4b-retroactive-signoff.md` — **APPROVED WITH CONDITIONS 7.0/10**, Security FAIL, Implementation Readiness FAIL, 7 conditions C1–C7 |

The retroactive sign-off (2026-04-17) is the binding record of review for Phase 4b and supersedes the 2026-04-16 proxy review. It closes the Golden Rule #11 gap for this phase and explicitly disallows cutting a public `v*` tag until conditions C1–C3 are closed.

---

## Exit Criteria (from spec §8)

| # | Criterion | Status | Notes |
|---|-----------|--------|-------|
| 1 | OfflineBanner in ChatView/InboxView/CalendarView/FilesView | **MET** | Wave 1.3 |
| 2 | NotesView banner when no active Claude session | **MET** | Wave 1.3 (inline notice, not banner; stricter than spec) |
| 3 | "Connect to Claude" disabled offline with spec-mandated message | **MET** | Wave 1.3/1.4 |
| 4 | `switchEngagement()` guarded same as `connect()` | **MET** | Wave 1.2 |
| 5 | Mid-session loss → "Connection interrupted" + retry | **MET** | Wave 1.4 |
| 6 | OAuth button disabled offline with "Sign in requires internet." | **MET** | Wave 1.5 |
| 7 | TasksView fully functional offline | **MET** | (No change needed — local SQLite, pre-existing) |
| 8 | `tauri-plugin-updater` integrated with signing keypair | **PARTIALLY MET** | Plugin integrated; keypair not generated (spec §4.5 human task; Codex C1) |
| 9 | CI produces GitHub Releases with update manifests on tag push | **PARTIALLY MET** | Workflow wired (Wave 2.6); never exercised with real keys; no hard-fail guard (Codex C3) |
| 10 | SettingsView shows version via `getVersion()` + check button | **MET** | Wave 2.4/2.5 |
| 11 | Update notification with "Install & Restart" | **MET** | Wave 2.4 |
| 12 | DMG background image with IKAROS branding + drag cue | **PARTIALLY MET** | Placeholder shipped (Wave 3.1; Codex C6) |
| 13 | DMG window size + icon positions configured | **MET** | Wave 3.2 |
| 14 | All existing tests (113) continue to pass | **MET** | 129 at HEAD |
| 15 | New tests for offline detection + update checker | **MET** | +16 JS tests (Waves 1.6, 2.7) |

**Inherited from Phase 4a (not 4b criteria, but prerequisites for shipping):**

- Apple Developer Program enrollment — **OUTSTANDING** (human task).
- Real Apple signing certificates in GitHub Secrets — **OUTSTANDING**.

---

## Known Carry-Over Work (Codex Retroactive Sign-off Conditions C1–C7)

Sourced verbatim from `.output/codex-reviews/2026-04-17-m2-phase4b-retroactive-signoff.md`. These are the outstanding items that must close before Phase 5 exit (C4–C5) or before the first public `v*` tag (C1–C3); C6–C7 are nice-to-have.

- [x] **C1 (Critical — Security, blocks release).** Generate updater keypair per spec §4.5 (`npx tauri signer generate -w ~/.tauri/ikrs-workspace.key`). Replace `"GENERATED_PUBLIC_KEY_HERE"` at `src-tauri/tauri.conf.json:29` with the real base64 ed25519 public key. Store private key + password in GitHub Secrets (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) only. Validate end-to-end with a throwaway tag against a staging repo before `v0.2.0`. **Closed 2026-04-17 per handoff instructions in this plan task.**
- [ ] **C2 (Critical — Availability, blocks release).** Confirm `IKAROSgit/ikrs-workspace` repo visibility. If private, move `latest.json` to GitHub Pages or a public CDN and update `tauri.conf.json:27` endpoint. Document choice in the release runbook.
- [ ] **C3 (Critical — CI hardening, blocks release).** Add a `tag`-gated step in `.github/workflows/ci.yml` that hard-fails when `TAURI_SIGNING_PRIVATE_KEY` is empty:
      ```yaml
      - name: Verify updater key present on release
        if: github.ref_type == 'tag'
        run: |
          [ -n "${TAURI_SIGNING_PRIVATE_KEY}" ] || { echo "Updater key missing"; exit 1; }
      ```
- [ ] **C4 (Important — Docs).** Produce `.output/2026-04-17-m2-phase4b-handoff.md` (Golden Rule #12). Add a "Distribution / Updates" chapter to `README.md`. Add an "Updater Trust Anchor + Key Rotation" section to `.architecture/SECURITY.md` that documents key storage ( `~/.tauri/ikrs-workspace.key` must be destroyed after upload to GitHub Secrets or stored only in a sealed password manager) and the coarse-grained updater capability posture.
- [x] **C5 (Important — Planning).** Write the retroactive plan file at `docs/superpowers/plans/2026-04-13-m2-phase4b-distribution-polish.md`. **This document closes C5.**
- [ ] **C6 (Nice-to-have).** Replace the 1689-byte placeholder DMG background with the final IKAROS-branded asset (logo + visual drag-arrow) before public release.
- [ ] **C7 (Nice-to-have — test coverage).** Add an updater downgrade-protection unit test (fixture `latest.json` with older version, assert client ignores) and a disabled-by-default E2E harness.

---

## Summary

| Wave | Spec Ref | Tasks | Files Touched | Tests Added | Status |
|------|----------|-------|---------------|-------------|--------|
| 1: Offline | §3.1–§3.6 | 1.1 – 1.6 | 1 new component + 6 view/hook modifications + 3 test files | 13 JS | Complete |
| 2: Auto-update | §4.1–§4.6 | 2.1 – 2.7 | 1 new component + 6 config/wiring edits + 1 CI edit + 1 test file | 4 JS | Complete (pubkey placeholder, see C1) |
| 3: DMG polish | §5.1–§5.3 | 3.1 – 3.3 | 1 new image + 1 config edit | 0 | Complete (placeholder art, no E2E build) |
| 4: Validation | §7 | n/a | n/a | n/a | Partial — DMG build not verified |

**Total:** ~16 logical tasks, 23 files, +1006 / −10, 129 tests, 1 squash commit (`b4bb80a`).

**Shipped state:** On `main`, retained per retroactive sign-off. **Not ship-ready to public users** until C1 (now closed), C2, and C3 are all closed. Phase 5 cannot exit until C4 and C5 (now closed) are both closed.

**Human/operational blockers retained from prior phases:** Apple Developer enrollment; production updater keypair handling in a sealed vault rather than the developer laptop.
