# Changelog

All notable changes to IKAROS Workspace are documented here. Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [SemVer](https://semver.org/).

## [Unreleased]

### Added — 2026-04-17 (Phase 4c in progress)

- `tools/scripts/local-ad-hoc-sign.sh` — one-command build + ad-hoc sign + install workflow. Lets the maintainer use the app on his own Mac while waiting on Apple Developer enrolment.
- `SECURITY.md` — secret inventory, updater key management, rotation procedure, release-readiness checklist, incident response.
- Tauri updater keypair generated. Public key `2363FF5A3F800DC9` embedded in `tauri.conf.json`; private key held outside repo and slated for GitHub Secrets upload.
- CI workflow (`.github/workflows/ci.yml`) now rejects `v*` tag pushes when required signing secrets are missing (`TAURI_SIGNING_PRIVATE_KEY`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`) or when the updater pubkey is still the placeholder.
- `docs/specs/m2-phase4c-release-readiness-design.md` — draft spec for the release-readiness phase.
- Retroactive governance artefacts: Phase 4b implementation plan, Phase 3b spec amendments, Phase 4a session handoff, plus three 2026-04-17 Codex retroactive sign-offs for Phases 3b/4a/4b.

### Changed

- `src-tauri/tauri.conf.json:29` — updater pubkey replaced placeholder `GENERATED_PUBLIC_KEY_HERE` with real value.
- Removed empty `src-tauri/src/mcp/` directory shell (Phase 3b retirement finally complete).
- `README.md` — replaced Tauri template boilerplate with the real product README.

## [0.0.1] — 2026-04-11 to 2026-04-13 (M1 + M2 Phases 1 through 4b)

First functional build of the app. Not a tagged release; shipped as local commits on `main`. Summarised from the phase-level handoffs at `.output/*-session-handoff.md` and phase plans under `docs/superpowers/plans/`.

### M1 — Consultant desktop app foundation (2026-04-10 / 2026-04-11)

- Tauri 2 + React 19 + TailwindCSS + shadcn/ui application shell.
- Firebase Auth (IKAROS identity) + Firestore (consultants, clients, engagements, tasks).
- Zustand state stores. PKCE OAuth flow for per-engagement Google account linking. OS keychain (via `keyring` crate) for refresh token storage.
- Seven views: Settings, Tasks, Inbox, Calendar, Files, Notes, Claude.
- Per-engagement Obsidian vault lifecycle (create / archive / restore).
- Error boundaries, offline detection, GitHub Actions CI.
- Codex Tier 3 milestone review: 7/10 PASS, all conditions addressed.

### M2 Phase 1 — Embedded Claude subprocess (2026-04-11)

- `session_manager.rs` — spawns Claude CLI as a monitored child process, streams its JSON output, maintains a session registry.
- `stream_parser.rs` — structured event extraction from Claude's stream.
- ChatView + chat components replace the placeholder ClaudeView.
- Codex review: 8/10 PASS, three conditions addressed.

### M2 Phase 2 — Skill system (2026-04-11)

- Per-engagement skill scaffolding under the vault directory, rejecting any path traversal outside `~/.ikrs-workspace/vaults/`.
- Skill sync (pull updates from a central source), skill update apply, unit tests for path safety.
- Codex review: PASS with minor conditions.

### M2 Phase 3a — Session UX (2026-04-11)

- Session resume via `--resume` flag, JSON session registry with atomic write + orphan cleanup.
- Engagement switch with session-detail dropdown, chat history partitioning per engagement (FIFO 50 messages).
- `useWorkspaceSession` orchestrator hook.
- Codex conditions C1/C2/I1 addressed.

### M2 Phase 3b — MCP wiring + token resilience (2026-04-12)

- Claude CLI owns MCP server lifecycle via `--mcp-config`. App-side `McpProcessManager` fully retired.
- Per-engagement `.mcp-config.json` generation at session spawn time.
- Google OAuth token expiry detection via stream parser, re-auth toast in ChatView with event-driven chain (kill session → OAuth → token-stored event → reconnect).
- Codex reviews: 9/10 PASS (proxy 2026-04-16) + 8/10 PASS with conditions (2026-04-17 second opinion); all conditions closed via 2026-04-17 amendments.

### M2 Phase 3c — MCP polish (2026-04-12)

- Strict MCP mode as engagement-level setting (for NDA clients).
- Auth-error stream detection re-keyed from `tool_id` to `tool_name` (latent 3b bug).
- mcpStore wired to `system.init` events for live server status.
- Codex review: PASS.

### M2 Phase 4a — Sandbox readiness + code signing scaffold (2026-04-12)

- App identifier migrated to `ae.ikaros.workspace` with data migration from legacy `com.*` path.
- OAuth refresh-token module with 5-minute expiry buffer.
- Binary resolver for `claude`/`npx`/`node` under macOS App Sandbox (avoids PATH dependency).
- macOS entitlements, capability restrictions, persisted-scope plugin.
- CI env-var plumbing for Apple signing + notarization (activates when secrets are set).
- Codex retroactive review (2026-04-17): 8/10 PASS with conditions. Apple Developer enrolment pending; notarized DMG not yet produced.

### M2 Phase 4b — Distribution polish (2026-04-13)

- `UpdateChecker.tsx` — user-visible update check flow via Tauri `plugin-updater`.
- `OfflineBanner.tsx` — network-loss UX across views.
- DMG window geometry + placeholder background.
- Updater config in `tauri.conf.json` (endpoint + placeholder pubkey — the placeholder is the ship-blocker resolved in Phase 4c).
- Codex retroactive review (2026-04-17): 7/10 PASS with conditions; 2 FAILs on Security + Readiness flagged the placeholder pubkey (since fixed) and absent Apple-signed smoke test (pending enrolment).

### Known limitations

- Apple Developer enrolment pending review. No notarized DMG has been produced. Daily use is via the ad-hoc sign script.
- Repo is private; `latest.json` hosting decision (make repo public vs. CDN) is a Phase 4c deliverable.
- ikrs-workspace vault paths currently at `~/.ikrs-workspace/vaults/`; ADR-013 (in the parent monorepo) moves these under the Google Drive shared path; migration is deferred to Phase 4d.

[Unreleased]: https://github.com/IKAROSgit/ikrs-workspace/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/IKAROSgit/ikrs-workspace/releases/tag/v0.0.1
