# Codex Review — M2 Phase 4b Final (Distribution Polish)

```
CODEX REVIEW
============
Subject: M2 Phase 4b — offline detection, auto-update, DMG polish
Type: phase-review (retroactive; Codex canvas offline, proxy via superpowers:code-reviewer per CODEX.md)
Date: 2026-04-16
Reviewed by: Codex (proxy)
Commit: b4bb80a feat: Phase 4b — offline detection, auto-update, DMG polish (main)
Parent: 01ef2a6 (Phase 4a wrap-up)

VERDICTS
--------
1. Structural:     PASS  — Wave 1/2/3 decomposition executed; plugin registered
                          in src-tauri/src/lib.rs, capability wired, JS plugin
                          added to package.json, UI in SettingsView.
2. Architecture:   WARN  — Offline strategy (banner + navigator.onLine guard) is
                          consistent, but offline detection relies entirely on
                          browser heuristic with no HTTP probe. Acknowledged in
                          P4b-R1 as MVP-acceptable.
3. Security:       FAIL  — Updater pubkey committed as literal
                          "GENERATED_PUBLIC_KEY_HERE" placeholder
                          (src-tauri/tauri.conf.json:29). Ship-blocking.
                          Plus: updater:default capability granted without
                          scoped endpoint allowlist; release endpoint URL
                          assumes a public repo with no fallback gating.
4. Completeness:   WARN  — Spec Section 4.5 "Signing Keypair (Human Task)" is
                          explicitly not done. README has no distribution /
                          update chapter. No handoff doc for Phase 4b in
                          .output/.
5. Risk register:  PASS  — P4b-R1..R5 all present and honest. R5 (keypair mgmt)
                          is exactly the finding above; documented, not fixed.
6. Spec alignment: PASS  — All Section 3 offline exit criteria implemented
                          verbatim (message strings match parent spec 3.15).
                          switchEngagement guard added per spec line 77.
                          DMG config matches spec 5.2 byte-for-byte.
7. Readiness:      FAIL  — Auto-update is non-functional until pubkey + CI
                          secrets are populated. Shipping a release now would
                          either fail signature verification on every client
                          or (worse) ship an unverifiable binary if pubkey
                          check is bypassed.

DECISION: APPROVED WITH CONDITIONS
Score: 7.0/10
```

## Conditions (must close before first public release)

**C1 (Critical, blocks release):** Generate the updater keypair per spec 4.5,
replace `"GENERATED_PUBLIC_KEY_HERE"` in `src-tauri/tauri.conf.json` with the
real ed25519 public key, and store the private key + password in GitHub Secrets
(`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). Private key
must never touch the repo. Until this lands, the updater endpoint is live-wired
but the trust anchor is a stringly-typed placeholder — a MITM attempt would be
detected by Tauri's sig verify against the placeholder and fail closed, but a
future dev who "just fills in any key to test" would open the supply-chain hole.

**C2 (Critical, blocks release):** Confirm `IKAROSgit/ikrs-workspace` repo
visibility. The updater endpoint
`https://github.com/IKAROSgit/ikrs-workspace/releases/latest/download/latest.json`
is a 404 for private repos without a token. Either make releases public or
move `latest.json` to a public CDN / GitHub Pages (P4b-R2 already flags this).

**C3 (Important):** Document the distribution + update story in README.md and
produce `.output/2026-04-13-m2-phase4b-handoff.md`. Golden Rule #12 requires
current docs; the commit message is not enough.

**C4 (Important):** Scope the `updater:default` capability to only the
configured endpoint host if Tauri permits (check tauri v2 updater scope
schema); otherwise document the wide permission and its risk profile in
`.architecture/SECURITY.md`.

**C5 (Nice-to-have):** Replace the 1689-byte placeholder dmg-background.png
with the final IKAROS-branded asset before public release (acknowledged in
P4b-R4).

## Detailed Findings

### 1. Offline detection + graceful degradation (Wave 1) — PASS

`src/hooks/useOnlineStatus.ts` is a correct `navigator.onLine` + online/offline
event bridge. `OfflineBanner` is wired into all five spec-listed views
(ChatView, InboxView, CalendarView, FilesView, NotesView) with the right
per-service copy. `useWorkspaceSession.connect()` and `switchEngagement()`
both short-circuit with the parent-spec-mandated string:

> "Unable to reach Claude. Check your internet connection and try again."

ChatView (line 154) correctly overrides error rendering when `!isOnline` with
the spec 3.15 "Connection interrupted. Your work is saved locally." text.
SettingsView disables the OAuth CTA with the exact tooltip
"Sign in requires internet." (line 237). Tests cover the hook, the banner,
and both offline guards (`tests/unit/hooks/useWorkspaceSession-offline.test.ts`).

**Minor gap:** Banner-in-ChatView renders twice in the file (lines 120, 134) —
once in the disconnected branch and once in the connected branch. Not a bug,
but a refactor target. No action required for 4b.

### 2. Auto-update (Wave 2) — FAIL on security

Wiring is correct: crate `tauri-plugin-updater = "2"` in Cargo.toml:26,
registered in `src-tauri/src/lib.rs:54`, JS peer `@tauri-apps/plugin-updater@^2.10.1`
in package.json, capability permission `updater:default` added in
`capabilities/default.json:20`. `UpdateChecker.tsx` uses `getVersion()` +
`check()` + `downloadAndInstall()` correctly with a sensible state machine
(idle/checking/available/downloading/error) and silent on-mount check.

**Blocking issue:** `tauri.conf.json:29` ships with
```
"pubkey": "GENERATED_PUBLIC_KEY_HERE"
```
This is the single most load-bearing string in the app's security posture.
A released binary with this placeholder will refuse all updates (fail-safe
by Tauri's verifier), so the failure mode today is "auto-update is broken,"
not "supply-chain RCE." But the commit ships this to `main` with CI already
passing `TAURI_SIGNING_PRIVATE_KEY` through to `tauri-action`, meaning the
next tag push will produce `latest.json` signed with a real private key that
no deployed client can verify. Worse: the pattern invites a future "just put
any pubkey in there to make it work" fix that silently opens the door.

**Required:** C1 above. Generate keypair, commit public half, store private
half in GitHub Secrets only. Validate with `tauri signer verify` against a
tagged test release before cutting v0.2.0.

**Related:** CI env (`ci.yml:100-101`) already passes the updater signing
secrets. Good — but the comment says "skipped gracefully when secrets not set"
which applies to Apple certs. The updater secrets will simply produce a
manifest signed with nothing and Tauri will reject it client-side. Add a
guard in the release job that hard-fails if `TAURI_SIGNING_PRIVATE_KEY` is
empty on a tag push, to prevent a silently-broken release.

### 3. DMG polish (Wave 3) — PASS

`src-tauri/icons/dmg-background.png` exists, 660x400, PNG, 1689 bytes
(placeholder as spec 5.1 allows). `tauri.conf.json:47-52` has the exact DMG
window size + icon coordinates from spec 5.2. Non-blocking on final art.

### 4. CI regression risk — PASS with note

`ci.yml` still runs lint, typecheck, npm audit high, vitest, cargo test,
and matrix build across macOS/Ubuntu/Windows. Phase 4a signing env vars
(`APPLE_*`) and Phase 3b/3c MCP tests are not removed or reshuffled. The
tag-gated release fields use ternary expressions to remain empty on PRs,
which is correct.

**Note:** `npm audit --audit-level=high` will now include
`@tauri-apps/plugin-updater` in its dep closure. No current advisories on
v2.10.1. Keep an eye on it.

### 5. Dependencies — PASS

Two new runtime deps: `tauri-plugin-updater = "2"` (Rust) and
`@tauri-apps/plugin-updater@^2.10.1` (JS). Both are first-party Tauri
crates under the Tauri MIT/Apache-2.0 license. Package-lock.json adds
10 lines, Cargo.lock adds 178 lines (transitive: reqwest, zip, etc. that
updater pulls in). Nothing exotic, nothing abandoned. Size impact on the
bundle is acceptable for an Electron-alternative.

### 6. Documentation (Golden Rule #12) — WARN

- Spec file `docs/specs/m2-phase4b-distribution-polish-design.md` status
  line reads "Implementation Complete (pending Apple Developer enrollment
  + updater keypair generation)" — honest, good.
- No handoff doc in `.output/` for Phase 4b. Prior phases produced one
  (see `.output/2026-04-11-m2-phase1-session-handoff.md`); 4b broke the
  pattern.
- README.md has zero mentions of distribution, updates, or `latest.json`.
  End users (future IKAROS consultants) have no docs telling them updates
  are automatic.
- `.architecture/SECURITY.md` should mention the updater trust anchor and
  the keypair rotation procedure. Currently silent.

### 7. Spec compliance table

| Exit criterion (spec §8)                                    | Status |
|-------------------------------------------------------------|--------|
| OfflineBanner in Chat/Inbox/Calendar/Files                   | PASS   |
| NotesView banner when no Claude session                      | PASS   |
| Connect to Claude disabled + correct message                 | PASS   |
| switchEngagement guarded                                     | PASS   |
| Mid-session loss "Connection interrupted..." + retry         | PASS   |
| OAuth button disabled + "Sign in requires internet."         | PASS   |
| TasksView functional offline                                  | N/A (not touched, presumed unchanged) |
| tauri-plugin-updater integrated with signing keypair          | **FAIL — pubkey placeholder** |
| CI produces GitHub Releases on tag push                       | PASS (pipeline exists; awaits real tag) |
| SettingsView shows version + Check for Updates                | PASS   |
| Update notification with Install & Restart                    | PASS   |
| DMG background with branding + drag cue                       | PARTIAL (placeholder) |
| DMG window/icon positions configured                          | PASS   |
| All 113 existing tests still pass, + new tests                | PASS (129 total per commit msg) |

## Security Summary (the critical lens)

The single biggest supply-chain surface in this commit is the auto-updater
trust path. That path has three links:

1. **Manifest transport:** GitHub Releases over TLS — OK.
2. **Manifest signature:** ed25519 signed by Tauri's private key held in
   GitHub Secrets — *configured but not yet populated; C1*.
3. **Client-side verification:** Public key baked into `tauri.conf.json`
   at build time — *placeholder string, C1*.

If (3) stays a placeholder through a public release, clients will reject
all updates (fail-closed, OK short-term, bad UX). If someone "fixes" (3)
without regenerating (2) and rotating the corresponding private key into
GH Secrets, you ship a release that passes verification with whatever key
they used — if that key ever leaks, an attacker can sign a malicious
`latest.json`. This is why the generation + populate step is a tied pair
and must be done together, documented, with the private key never existing
outside the secrets vault (no local backup, or if local-backed-up: in a
sealed password manager, not `~/.tauri/`).

Recommend: before the first `v*` tag push, run a dry-run release against a
staging repo, verify the update flow end-to-end with a throwaway keypair,
then generate production keys and re-run.

## Codex Proxy Note

Codex canvas was offline during this review. Per CODEX.md §"tldraw Canvas"
and Golden Rule #11, `superpowers:code-reviewer` is the authorized proxy
channel when the canvas is unavailable. This review applies the 7-point
validation verbatim and produces a binding verdict with conditions.

End of review.
