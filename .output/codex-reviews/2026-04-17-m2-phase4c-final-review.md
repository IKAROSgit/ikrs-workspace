# Codex Final Checkpoint — M2 Phase 4c (commit `45ebb3d`)

**Reviewer:** Codex
**Date:** 2026-04-17
**Scope:** Every file changed in commit `45ebb3d` — "feat(phase4c): release readiness — downgrade protection, smoke test, daily-use tooling."
**Previous review:** `.output/codex-reviews/2026-04-17-bundle-remediation-phase4c-signoff.md` (PROCEED WITH CONDITIONS, 8.5/10 on spec)
**Verdict:** **PASS WITH TWO MINOR CONDITIONS** — 9/10.

---

## Daily-use verdict

**Safe to start using daily, now.** Run `./tools/scripts/local-ad-hoc-sign.sh install` on your Mac and use the `.app` from `/Applications`. Nothing in this commit introduces a sharp edge for single-user local use. The two minor conditions below are documentation defects and a missing integration test — they do not touch runtime correctness on your machine.

---

## 7-point protocol

### 1. Structural — PASS

The downgrade helper (`src/lib/version-compare.ts:1-75`) is cleanly a leaf module. `UpdateChecker.tsx:5` imports `isNewerVersion` and uses it through its public API at two call sites (`UpdateChecker.tsx:34` and `:67`). `parseVersion` is correctly kept internal via `_internals` at `version-compare.ts:74`. No coupling leak — a refactor of `UpdateChecker` would not ripple into the helper, and vice versa.

The new `tools/scripts/` directory is the first of its kind in the repo; the naming + placement is consistent with the rest of the project structure.

### 2. Architecture — PASS

**Layer 2 defence-in-depth is correctly implemented.** Two call sites (`UpdateChecker.tsx:34` pre-accept, `:67` pre-install) re-check `isNewerVersion(update.version, current)` after Tauri's `plugin-updater` has already done its own comparison. The "final re-check before `downloadAndInstall()`" at `:67` is the valuable one — it guards against stale React state where the user sees an "available" button that was set under different conditions (e.g. if version state changed underneath, or if the user leaves the Settings view open for hours). Both checks fall back to `getVersion()` when local `version` state is empty (`:33`, `:66`), closing the "if current is unknown, refuse to update" path from the spec.

The smoke-test workflow (`.github/workflows/smoke-test.yml:1-136`) does what it claims: builds a real DMG from clean clone on `macos-latest`, mounts it, installs to `/Applications`, launches headless, asserts 10s stability, greps `DiagnosticReports` for crashes in the last 5 minutes. Teardown (`:125-135`) runs on `always()` so a failing assertion still unmounts + quits. Solid.

One architectural note: the smoke test's ad-hoc re-sign step (`smoke-test.yml:82-91`) comments "in production the DMG will be real-signed and this step becomes a no-op" — but the step doesn't actually become a no-op automatically; it unconditionally `codesign --force --deep --sign -` on whatever is in `/Applications`. For the CI-only smoke test that's fine, but if this workflow is ever adapted to gate a real release the step needs to be conditional on the build type. Not a blocker; call it a future-maintenance note.

### 3. Security — PASS with one minor doc defect

**`parseVersion` rejects every malformed input the test asserts.** I re-ran `npx vitest run tests/unit/lib/version-compare.test.ts` — all 31 cases green including the explicit `null`/`undefined` cases at `version-compare.test.ts:98-103`. The regex `/^\d+$/` at `version-compare.ts:42` catches every non-digit including `-`, `a`, empty segments, and scientific notation. Negative numbers are correctly rejected because the `-` is split off as a pre-release suffix *after* the `v` prefix strip (see the comment at `version-compare.test.ts:93-96`) — the first segment then becomes empty and `p.length === 0` fails.

**Layer 2 guard bypass analysis:** The only paths into `downloadAndInstall()` are through `handleInstall` (`UpdateChecker.tsx:61-79`), which re-checks before proceeding. `update` state is only set via `setUpdate(result)` at `:41`, which is gated by the Layer 2 check. No other code path sets `update`. No bypass.

**CI guard matches `SECURITY.md` promises.** The new guard at `ci.yml:77-107` runs on every matrix OS (B-C1 closed), checks `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` presence via `${+set}` semantics (B-C2 closed; this is the correct idiom for "defined but possibly empty"), and only checks Apple secrets on macOS which matches the actual distribution topology. The "defined but empty" semantics match `SECURITY.md:46` which explicitly states "The secret must be defined even when empty."

**`local-ad-hoc-sign.sh` shell-injection analysis:** `set -euo pipefail` at `:18`. `SCRIPT_DIR` and `REPO_ROOT` use `$(cd "..." && pwd)` with proper quoting at `:21-22`. `APP_PATH` is a literal string concat with a fixed bundle name (`:44`). The only external input is `${1:-}` at `:72`, compared against the literal `"install"` — no interpolation into a shell command. `codesign`, `xattr`, `cp`, `rm -rf` all receive quoted args. `du -sh | cut -f1` at `:67` has a fixed input. **Clean.**

**Smoke test secret leakage analysis:** The workflow env at `smoke-test.yml:44-53` uses hardcoded `ci-placeholder` values, not GH Secrets. No `secrets.*` references anywhere in the file. No `printenv`, no unredacted `set -x`, no artifact upload of build logs. The step that prints crash reports (`:107-108, :117-121`) inlines file contents but those are OS crash dumps, not env. **No leak path.**

**Minor doc defect (Condition F-C1):** `SECURITY.md:112` lists the OAuth-token-refresh site as `src-tauri/src/commands/oauth/token_refresh.rs`. The actual path is `src-tauri/src/oauth/token_refresh.rs` — the file lives under `src-tauri/src/oauth/`, not under `src-tauri/src/commands/oauth/`. The audit's substantive claim (zero webview-JS callers of keyring — confirmed by my `grep`) is correct, but the file-path column is wrong and will confuse a future auditor trying to walk the table. One-line fix.

### 4. Completeness — WARN (one genuine gap)

**Task 16 from the plan is silently incomplete.** `docs/superpowers/plans/2026-04-17-m2-phase4c-release-readiness.md:43` lists Task 16: *"`tests/unit/components/UpdateChecker.test.tsx` — expand to cover Layer 2 rejection of same + lower version."* Commit `45ebb3d` does **not** touch `UpdateChecker.test.tsx`. I verified with `git show 45ebb3d -- tests/unit/components/UpdateChecker.test.tsx` — no output. The 4 existing tests in that file (`tests/unit/components/UpdateChecker.test.tsx:19-68`) are all Phase 4b vintage. There is no test that, given an `update` object with `version: "0.1.0"` and `getVersion()` returning `"0.1.0"`, the component does NOT render "Update available." There is also no test that `handleInstall` rejects a stale `update` at the second call site.

The unit-level `version-compare` coverage is excellent (31/31), so the business logic is proven. What's missing is the *integration* proof that `UpdateChecker` actually calls `isNewerVersion` in the right places. If a future refactor accidentally removed the `:34` or `:67` guard, the test suite would still be green.

**Recommended subtests to add:**
- `"silently rejects update when same version" `: mock `check()` returning `{version: "0.1.0", ...}` with `getVersion()` returning `"0.1.0"` — assert no "Update available" text.
- `"silently rejects downgrade when older version" `: mock `check()` returning `{version: "0.0.9", ...}` — assert no "Update available" text.
- `"rejects install on stale state"`: render with an update available, then somehow (re-render with new `getVersion` mock) assert the install button refuses to call `downloadAndInstall`.

Other completeness items all green: `SECURITY.md:104-126` keychain audit is present and accurate modulo the one path defect; `docs/decisions/2026-04-17-latest-json-hosting.md` is a real A/B doc with concrete prerequisites; `CHANGELOG.md` covers M1 through 4c honestly; README replaces boilerplate.

**Success-criteria checklist honesty** (`docs/superpowers/plans/2026-04-17-m2-phase4c-release-readiness.md:84-93`): The checklist correctly has the downgrade-protection item marked unchecked because Moe hasn't re-run it post-commit. However, given Task 16 is silently dropped, the "Downgrade protection ≥5 test cases" criterion is actually under-met at the integration level — the version-compare suite passes all 31 cases, but the spec §3 success criteria asks for ≥5 cases *on the actual UpdateChecker logic* (see bundle review §C "Tests: …all assertions on the actual UpdateChecker logic"). Technically met by the existence of the helper tests; spiritually not.

### 5. Risks — PASS

The hosting decision doc (`docs/decisions/2026-04-17-latest-json-hosting.md`) is actually useful: concrete prerequisites for Option A (`gitleaks` + `trufflehog` pre-flight), real cost estimates for Option B, explicit recommendation with fallback criteria. The "no-op until Moe picks" posture at `:129-131` correctly prevents rogue endpoint changes.

**New risks introduced by Layer 2 guard — false-positive analysis:**
- `isNewerVersion("1.2.3-hotfix.1", "1.2.3") === false` means a legitimate hotfix pre-release would be rejected if the user is on the stable `1.2.3`. This is intentional per the header comment (`version-compare.ts:16-19`) — pre-release channels should not ship through stable auto-update. Correct posture for this app (single-tenant, no beta channel).
- `isNewerVersion("1.2", "1.1.99") === true` because missing segments default to 0. If we ever ship `1.2` without explicit patch, that's fine; if an attacker fabricates `"2"` claiming to be `2.0.0`, Tauri's bundle signature check gates the actual install so the Layer 2 false-positive-as-newer doesn't matter — a malicious payload can't get past signature verification.
- No case where a legitimate update is false-rejected that I can construct.

### 6. Spec/code alignment — PASS with caveat

Wave 1: daily-use script + README + CHANGELOG — all present and matching spec §5.
Wave 2: CI fix (B-C1), password check (B-C2), SECURITY.md rsign2 naming (B-C3), spec §3 + §7 amendments (C-C1, C-C2) — all closed, verified by reading each file against the previous review's conditions.
Wave 3: version-compare helper + UpdateChecker integration + hosting decision + keychain audit — all present. **Task 16 integration test is the one item marked Done in the commit message that isn't actually done** (commit doesn't touch that file, but the commit message doesn't list it either — it is marked "Pending" at plan line 43, which is accurate; so no spec-vs-code misalignment claim, just incompleteness against the original plan scope).
Wave 4: `smoke-test.yml` — present; spec §6 said "clean-machine install + launch verification on macos-latest" and that's what it does.

**Caveat:** the commit message at lines 28-29 says "31 cases covering basic ordering, v-prefix, pre-release, missing segments, malformed input, and explicit attack scenarios" — this matches the helper-level tests but overstates Layer 2 *integration* coverage. Minor.

### 7. Readiness — PASS

Daily-use readiness: **fully green.** Script is executable (`-rwxr-xr-x` verified), handles missing Tauri CLI, handles missing bundle dir, verifies signature before declaring success, strips quarantine, supports both build-only and build+install modes. Clear user-facing messaging. Exit codes correct (`2` for preflight, `1` for build/verify failure). First-launch right-click-Open instruction correct for ad-hoc signed apps on Sonoma/Sequoia.

`v0.1.0` tag readiness: blocked on external items (see below), not on code quality.

---

## What's left before `v0.1.0` tag

### Blockers on Moe (user-action, not code)

1. **Upload `TAURI_SIGNING_PRIVATE_KEY` to GH Secrets** — paste contents of `~/.tauri/ikrs-workspace.key`.
2. **Upload `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to GH Secrets** — set to empty string for current password-less key. The CI guard will fail cleanly if you forget.
3. **Pick Option A or B** for `latest.json` hosting per `docs/decisions/2026-04-17-latest-json-hosting.md`. If A, run `gitleaks detect --no-git -v` over repo history and address hits before flipping visibility. If B, create the GCS bucket and add `GCS_WRITE_SA_KEY`.
4. **Run a `test-signing-*` branch push** after step 1-2 to prove round-trip signing works (spec §1).

### Blockers on Apple (external, no ETA)

5. **Apple Developer enrolment** — no notarized DMG until this completes. External consultants cannot install until then. Daily personal use is not blocked.

### Blockers on code (neither user nor Apple — this agent's todo)

6. **Task 16: `UpdateChecker.test.tsx` integration tests for Layer 2.** See §4 above for the three specific subtests. Estimated effort: 30-45 minutes in a small follow-up commit.
7. **F-C1: Fix `SECURITY.md:112` path.** Change `src-tauri/src/commands/oauth/token_refresh.rs` to `src-tauri/src/oauth/token_refresh.rs`. One-character edit in a row of a table. Estimated effort: 2 minutes.
8. **DMG background art (spec §4).** Marked deferred in commit message; needs a Mac design session. Not a `v0.1.0` ship-blocker strictly — a placeholder DMG ships the same binary — but is a polish item the spec lists in scope.

---

## Quality bar against the 2026-04-17 bundle review

Condition tracking:

| Prior condition | Status | Evidence |
|---|---|---|
| B-C1 (drop macOS-only filter on CI secret guard) | **CLOSED** | `ci.yml:78` — `if: github.ref_type == 'tag'`, macOS-only filter gone. Apple checks are correctly re-gated inside the step at `:94-98`. |
| B-C2 (check `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` presence) | **CLOSED** | `ci.yml:89-91` — uses `${+set}` semantics to distinguish unset from empty. |
| B-C3 (rsign2 naming) | **CLOSED** | `SECURITY.md:27` — corrected. |
| C-C1 (clarify Layer 2 as native `version` field, not custom) | **CLOSED** | `docs/specs/m2-phase4c-release-readiness-design.md:72` — amended with the exact wording. |
| C-C2 (coordinate §7 with Phase 4d) | **CLOSED** | `docs/specs/m2-phase4c-release-readiness-design.md:140` + `SECURITY.md:122` both explicitly state the coordination. |
| C-C3 (README note on Gatekeeper mode) | **NOT ADDRESSED** (was cosmetic/suggestion). README `Daily Use` section at `README.md:11-28` describes the right-click→Open flow but does not mention the "App Store only" Gatekeeper mode edge case from the prior review §C. Low-value. Leave. |

**Prior conditions closed: 5/6 (the 6th was cosmetic/suggestion).** Quality bar held.

---

## Summary

Moe: the app is safe to use daily as of `45ebb3d`. Run the install script and go. Before tagging `v0.1.0` publicly you need to upload two GitHub Secrets, pick an A/B hosting path, and wait on Apple — none of which is code work. I'd like to see the Task-16 integration tests and the one SECURITY.md path typo fixed in a small follow-up commit before the tag, but nothing about your daily use is blocked by either of those.

**Verdict: PASS with two minor conditions (integration tests + one doc typo). 9/10.**

Reviewed by: Codex
Date: 2026-04-17
