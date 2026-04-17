# Codex Retroactive Sign-off — M2 Phase 4b (Distribution Polish)

```
CODEX REVIEW (RETROACTIVE SIGN-OFF)
===================================
Subject:   M2 Phase 4b — offline detection, auto-update, DMG polish
Type:      retroactive 7-point sign-off (Golden Rule #11 gap-closure)
Date:      2026-04-17
Reviewer:  Codex
Commit:    b4bb80a  feat: Phase 4b — offline detection, auto-update, DMG polish
Parent:    01ef2a6  (Phase 4a wrap-up)
Spec:      docs/specs/m2-phase4b-distribution-polish-design.md (270 lines)
Plan:      NOT FOUND  (docs/superpowers/plans/ has no 4b plan file)
Prior rvw: .output/codex-reviews/2026-04-16-m2-phase4b-final-review.md (APPROVED W/ CONDITIONS, 7.0/10)
```

## Process Finding (Golden Rule #11)

This work shipped to `main` as a single squash commit authored 2026-04-13
11:50 +0400. The proxy review dated 2026-04-16 acknowledged it was performed
**after** the commit landed. There is therefore **no pre-merge Codex sign-off
on record** for b4bb80a. Today's review closes that gap and supersedes the
proxy review as the binding sign-off of record.

A secondary process finding: **no Phase 4b plan exists** in
`docs/superpowers/plans/`. The last plan is `2026-04-12-m2-phase4a-...`.
Phase 4b was implemented directly from the design spec with no executable
plan document. This is a Golden Rule #10 (planning discipline) shortfall.

## VERDICTS

| # | Point                    | Verdict | Summary |
|---|--------------------------|---------|---------|
| 1 | Structural validation    | PASS    | Three subsystems cleanly separated; no cross-coupling. |
| 2 | Architectural consistency| WARN    | Offline detection is `navigator.onLine` only — no HTTP probe, captive portals not handled; documented as MVP risk. Update flow has no explicit rollback path. |
| 3 | Security audit           | **FAIL**| Updater `pubkey` shipped as literal `"GENERATED_PUBLIC_KEY_HERE"`; capability scope is unrestricted; no downgrade protection documented; no hard-fail guard in CI if signing key missing on tag push. |
| 4 | Completeness             | WARN    | Placeholder pubkey, placeholder DMG art, spec §4.5 human task not executed, no DMG end-to-end build verification artifact attached. |
| 5 | Risk register            | PASS    | P4b-R1..R5 present and honest; R5 predicts C1 exactly. |
| 6 | Spec/code alignment      | PASS    | All spec §8 exit criteria implemented verbatim except pubkey population (explicit spec carve-out) and final DMG art (explicit spec carve-out). |
| 7 | Implementation readiness | **FAIL**| Not ship-ready. First `v*` tag push will produce a release whose updater manifest cannot be verified by any deployed client — or worse, will invite a future "fill in any key" patch that opens a supply-chain door. |

**DECISION: APPROVED WITH CONDITIONS (conditional sign-off)**
**Score: 7.0 / 10** (unchanged from proxy review; conditions identical)

The code merged to `main` is retroactively acceptable **only** because the
current pubkey placeholder produces fail-closed behavior in Tauri's verifier
(all updates rejected client-side). That makes the hazard a **release
readiness failure, not a runtime exploit**. Before cutting any `v*` tag,
C1–C2 below must land.

---

## Per-Point Detail

### 1. Structural validation — PASS

Wave 1 (offline): `OfflineBanner.tsx` (17 LOC, pure presentation), wired
into 5 views. Connect/switch guards live in `useWorkspaceSession.ts`
(lines 41–46, 104–109). Mid-session override in `ChatView.tsx:153–155`.
Wave 2 (updater): `UpdateChecker.tsx` (96 LOC) with idle/checking/available/
downloading/error state machine. Wave 3 (DMG): config block in
`tauri.conf.json:47–52`. No cross-wave coupling; each subsystem independently
deletable.

### 2. Architectural consistency — WARN

Spec → code mapping is faithful. Gaps vs. a "hardened" shipping app:

- **Offline detection coverage:** `navigator.onLine` does not detect
  captive portals, DNS failures that still return IP routability, or
  API-only outages (Anthropic up but user's ISP blocks it). Spec
  acknowledges this in P4b-R1 as MVP-acceptable. Acceptable for sign-off;
  follow-up ticket recommended.
- **Auto-update rollback:** No staged/atomic rollback path if the replacement
  `.app` bundle fails to relaunch. Tauri updater overwrites in place on
  macOS via `NSAppleScript` elevation. If the new binary crashes on first
  launch, the user has no "revert to previous" affordance. Parent spec does
  not mandate one; flagging for post-MVP.
- **ChatView double-banner:** `OfflineBanner` renders in both the
  disconnected branch (line 120) and the connected branch (line 134). Not
  a bug — only one branch is live at a time — but a minor refactor target.

### 3. Security audit — FAIL (ship-blocking on first release)

Auto-update is the single most dangerous surface in any desktop app. The
trust path has three links; (3) is broken:

1. **Manifest transport.** GitHub Releases over TLS. OK.
2. **Server-side signing.** `TAURI_SIGNING_PRIVATE_KEY` + `_PASSWORD` piped
   to `tauri-action` via `ci.yml:100–101`. Secrets-scoped, never echoed.
   OK *if populated*; current state unknown.
3. **Client-side verification.** `tauri.conf.json:29` ships:
   ```json
   "pubkey": "GENERATED_PUBLIC_KEY_HERE"
   ```
   This is a literal string, not a base64-encoded ed25519 key. Tauri's
   updater verifier will reject every signed manifest against this anchor
   (fail-closed → auto-update non-functional but not exploitable). The
   hazard is not RCE *today* — the hazard is the invitation to a future
   dev who "just fills in any key to make it work" and does so without
   rotating the corresponding private key into Secrets, creating a
   silent supply-chain hole.

Additional security concerns:

- **Endpoint pinning.** Only one endpoint is configured
  (`github.com/IKAROSgit/ikrs-workspace/releases/latest/download/latest.json`).
  URL is hard-coded, not env-driven. OK. But repo visibility is unverified
  (P4b-R2) — if repo is private, endpoint 404s and updater silently stalls.
- **Capability scoping.** `capabilities/default.json:20` grants
  `updater:default` to the main window with no URL-scope allowlist. Tauri v2
  updater scope is coarse (allow/deny), not per-host, so this is the
  available posture — but it should be called out in `.architecture/SECURITY.md`.
- **Downgrade protection.** Tauri's default updater compares versions and
  refuses older versions, but there is no explicit test or documentation
  confirming this behavior for this build. Recommend a unit/integration test
  that feeds a stale `latest.json` and asserts the client ignores it.
- **CI hard-fail missing.** `ci.yml` does not fail the release job if
  `TAURI_SIGNING_PRIVATE_KEY` is empty on a tag push. Add:
  ```yaml
  - name: Verify updater key present on release
    if: github.ref_type == 'tag'
    run: |
      [ -n "${TAURI_SIGNING_PRIVATE_KEY}" ] || { echo "Updater key missing"; exit 1; }
  ```
- **Key storage.** Spec §4.5 says private key lands at
  `~/.tauri/ikrs-workspace.key`. That file must be destroyed after it
  is uploaded to GitHub Secrets, or stored only in a sealed password
  manager — not left on a developer laptop. Document in SECURITY.md.
- **Identifier migration.** `lib.rs` still carries `migrate_app_data` from
  `com.moe_ikaros_ae.ikrs-workspace` to `ae.ikaros.workspace`. Unrelated to
  Phase 4b but note: if an old user auto-updates into a new identifier,
  the old data dir is ported, which is correct behaviour.

### 4. Completeness — WARN

- Offline + UpdateChecker UI: complete, tested (OfflineBanner 4 tests,
  UpdateChecker 4 tests, useOnlineStatus 5 tests, useWorkspaceSession
  offline 4 tests per file listing).
- Auto-update: wiring complete; **trust anchor not populated** (spec §4.5
  is the documented human task).
- DMG: bundle config in place; background image is a 1689-byte 660×400 PNG
  placeholder (spec §5.1 allows this); DMG has never been built, signed,
  notarized, and installed end-to-end in CI with actual Apple Developer
  credentials (Phase 4a left those as unset-skips-gracefully).
- Tests: 129 total (58 Rust + 71 JS) per commit message. No E2E test for
  the update path.
- Docs: no handoff doc in `.output/` for Phase 4b; README has no distribution
  chapter.

### 5. Risk register — PASS

P4b-R1 (captive portal), R2 (private repo endpoint), R3 (macOS elevation),
R4 (placeholder DMG art), R5 (keypair mgmt) all honest and matched to
mitigations. R5 correctly anticipates the pubkey blocker surfaced in C1.

### 6. Spec/code alignment — PASS

Spot-checks against the spec:

| Spec ref                    | Code location                                  | Match |
|-----------------------------|------------------------------------------------|-------|
| §3.2 banner wording          | `OfflineBanner.tsx:14`                         | Exact |
| §3.3 per-view routing        | Chat/Inbox/Calendar/Files/Notes views          | All 5 wired |
| §3.4 connect guard text      | `useWorkspaceSession.ts:41–46`                 | Exact |
| §3.4 switchEngagement guard  | `useWorkspaceSession.ts:104–109`               | Exact |
| §3.5 mid-session text        | `ChatView.tsx:153–155`                         | Exact |
| §4.2 plugin registration     | `lib.rs:54`, `Cargo.toml:26`, `capabilities:20`| Exact |
| §4.3 UpdateChecker component | `UpdateChecker.tsx`                            | Exact |
| §4.4 CI tag trigger          | `ci.yml:6–7`, `ci.yml:77–81`                   | Exact |
| §5.2 DMG config              | `tauri.conf.json:47–52`                        | Byte-for-byte |

### 7. Implementation readiness — FAIL

- **Dev / PR builds:** Safe. Updater endpoint is not exercised.
- **Ship-to-users readiness:** Not ready. First public release requires
  C1 and C2 closed. Without them, the release either breaks auto-update
  client-side (best case) or lays the groundwork for a supply-chain
  incident (worst case).

---

## Conditions (must close before v0.2.0 tag)

**C1 (Critical, blocks release — Security).** Generate the updater keypair
per spec §4.5; replace `"GENERATED_PUBLIC_KEY_HERE"` in
`src-tauri/tauri.conf.json:29` with the real base64 ed25519 public key;
store the private key and password in GitHub Secrets
(`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) and
nowhere else. Validate end-to-end with a throwaway tag against a staging
repo before cutting v0.2.0 on production.

**C2 (Critical, blocks release — Availability).** Confirm
`IKAROSgit/ikrs-workspace` repo visibility. If private, move `latest.json`
to GitHub Pages or a public CDN and update the endpoint in
`tauri.conf.json:27`. Document the choice in the release runbook.

**C3 (Critical — CI hardening).** Add a `tag`-gated step in `ci.yml` that
hard-fails if `TAURI_SIGNING_PRIVATE_KEY` is empty. Prevents silently
shipping a release whose manifest no client can verify.

**C4 (Important — Docs).** Produce `.output/2026-04-17-m2-phase4b-handoff.md`
(or equivalent) per Golden Rule #12. Add a Distribution / Updates chapter
to README.md. Add an "Updater Trust Anchor + Key Rotation" section to
`.architecture/SECURITY.md`.

**C5 (Important — Planning).** Add a retroactive plan file
`docs/superpowers/plans/2026-04-13-m2-phase4b-distribution-polish.md` so
the planning trail is not broken for Phase 5+.

**C6 (Nice-to-have).** Replace the 1689-byte placeholder DMG background
with the final IKAROS-branded asset before public release.

**C7 (Nice-to-have — test coverage).** Add an updater downgrade-protection
unit test and a disabled-by-default E2E test harness that stands up a
fixture `latest.json`.

## Blockers (absolute; do not cut a release until cleared)

- C1: pubkey placeholder
- C2: endpoint reachability unconfirmed
- C3: CI release-guard missing

## Strengths

- Offline detection UX is clean, spec-accurate, and well-tested. Exact
  wording from parent spec §3.15 is preserved verbatim.
- UpdateChecker state machine is minimal and correct; silent on-mount
  check degrades gracefully on error.
- Spec-to-code traceability is the strongest of any phase to date —
  every spec clause has a matching code line.
- Risk register honestly names the exact conditions surfaced in this
  review (R5 → C1).
- Tauri plugin integration follows the documented three-touch pattern
  (Rust crate + JS peer + capability) without deviation.
- CI extension is additive, ternary-guarded, and non-disruptive to PR
  builds.

## Decision Summary

**APPROVED WITH CONDITIONS — 7.0/10.**

The code on `main` is acceptable to retain. The Golden Rule #11 gap is
closed by this retroactive sign-off. **No public release (no `v*` tag
push) is authorized until C1, C2, and C3 are closed.** C4 and C5 must
close before Phase 5 exit. C6 and C7 may close alongside v0.2.0 or in a
fast-follow.

Codex explicitly flags this as a one-time retroactive sign-off.
Future phases must have a pre-merge Codex review attached to the PR
before main-merge, per Golden Rule #11.

End of review.
