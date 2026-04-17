# Codex Sign-Off — 2026-04-17 Remediation Bundle + Phase 4c Spec

**Verdict per artefact:**
- Artefact A (ADR-013): **PASS** — 9/10
- Artefact B (Security remediation `a00c46d`): **WARN** — 7.5/10 (two CI-guard defects + one doc inaccuracy; none block daily local use)
- Artefact C (Phase 4c spec): **PASS WITH CONDITIONS** — 8.5/10

**Overall:** **PROCEED WITH CONDITIONS.** Moe can start daily use of the app via `local-ad-hoc-sign.sh` **today**. The CI-guard defects only bite on the first real `v*` tag push, not on local builds.

## Summary

This review covers the ADR-013 Obsidian-path decision (`ikaros-platform` commit `2466c6a`), the security remediation bundle that generated the updater keypair and added CI guards + `SECURITY.md` (`ikrs-workspace` commit `a00c46d`), and the Phase 4c release-readiness spec. The ADR is architecturally sound and honest about its trade-offs. The security bundle substantively closes the Phase 4a/4b ship-blockers (pubkey placeholder gone, rotation procedure documented, CI checks added) but has two real bugs in the CI guard and one wrong claim in `SECURITY.md` about the keypair format. Phase 4c scope is correct and tightly focused; one cross-cutting item (Tauri `capabilities/default.json` still references the deprecated vault path after ADR-013) should be acknowledged as a 4d dependency. None of these block daily app usage — they block the first public tag push, which is weeks away pending Apple enrolment anyway.

## Artefact A — ADR-013 (`/home/moe_ikaros_ae/ikaros-platform/.architecture/DECISIONS.md` lines 320-395)

1. **Structural:** PASS — Three vault classes cleanly decomposed, each with one canonical path. Deprecation list is explicit.
2. **Architecture:** PASS — Elara-on-VM vs. Mac-on-Drive split is correct. GDrive FUSE write-conflict concern for a hot-path MCP writer is a real, documented failure mode; keeping Elara local is the right call. Consultant vaults on Shared Drive genuinely does solve the line-manager-visibility M2 requirement without an auth surface.
3. **Security:** PASS — Drive ACL-based visibility is auditable and revocable; no new secret material introduced.
4. **Completeness:** PASS — Consequences section is unusually honest (sandbox entitlement impact, CloudStorage path-with-spaces tolerance, multi-writer conflict risk in M3 all called out).
5. **Risk register:** PASS — Trade-offs enumerated; the macOS sandbox long-path-with-spaces validation is correctly flagged as Phase 4d work, not hand-waved.
6. **Spec alignment:** PASS — Deferring the mechanical migration to a new Phase 4d is the right split given it needs Moe's physical Mac and cross-spec amendments (M1 design, 3b, 4a persisted-scope).
7. **Readiness:** PASS — Phase 4d implementation steps (1-7 in the ADR) are concrete enough to plan from.

**Condition:** None. One observation: the ADR notes Phase 4d "must run before any external consultant installs the app." Ensure M2 phase-completion criteria include "Phase 4d done OR installer disables external-consultant path." Not a blocker here, but a future Codex gate.

## Artefact B — Security remediation (`ikrs-workspace` commit `a00c46d`)

1. **Structural:** PASS — Four-file change is minimal and focused; pubkey replacement, CI guard, `SECURITY.md`, `.gitignore` update.
2. **Architecture:** PASS — Key stored outside repo at `~/.tauri/ikrs-workspace.key` (perms 600); `.tauri/` is already in the CLAUDE.md Golden Rule #1 exception list per commit `2466c6a`. Correct.
3. **Security:** **WARN** — Three issues:
   - **CI-guard bug #1 (important):** `.github/workflows/ci.yml:77` gates the signing-secrets check on `github.ref_type == 'tag' && runner.os == 'macOS'`. The matrix builds on Linux too — a tag push with missing secrets will **pass** the Linux job and only fail macOS. Since `tauri-action` runs per-OS and signing happens on macOS, the release artefact still won't sign, but the CI overall-success signal is misleading: a partial-green PR status can mask the failure. Recommend dropping the `runner.os == 'macOS'` filter or moving the secret check into a dedicated single-runner gating job that runs before the build matrix.
   - **CI-guard bug #2 (important):** `SECURITY.md` lists `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` as required (line 41 of the file), but the CI guard does not check for its presence. If Moe leaves that secret unset but the private key file has a password, CI will fail later during `tauri-action` with a less-clear error. Either add it to the `missing=()` array or document that current key has no password so the secret must be explicitly set to empty-string in GitHub (which the current key does — but a rotation could change this silently).
   - **Doc inaccuracy (cosmetic):** `SECURITY.md` line 27 says the key format starts with `dW50cnVzdGVkIGNvbW1lbnQ6IHJzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5Cg…` — that decodes to "rsign encrypted secret key" even though the doc calls the tool "minisign (Ed25519)". It's `rsign2` (Tauri's Rust minisign). Rename for accuracy or leave; doesn't affect function.
4. **Completeness:** PASS — `SECURITY.md` covers: secret inventory, updater key management, rotation (including the critical transition-release dance in step 2 — this is correctly described and is the subtle bit most rotation docs get wrong), release checklist, incident response, open risks. One gap worth noting but not blocking: no mention of how to verify the committed pubkey matches the private key held in `~/.tauri/ikrs-workspace.key` (e.g., `npx tauri signer sign --password '' -k <key> <dummy> && minisign -V -p <pubkey> …`). Without that, the round-trip check is deferred to Phase 4c §1 — acceptable.
5. **Risk register:** PASS — Open risks section lists the real remaining blockers (Apple cert, repo-public-for-`latest.json`, permissive `keyring:default`).
6. **Spec alignment:** PASS — Addresses Phase 4b Codex condition C4 (SECURITY.md) and the pubkey placeholder FAIL.
7. **Readiness:** WARN — Guard bugs above need fixing before tagging `v0.1.0`, not before daily use.

**Conditions (fix before first real tag push, NOT before daily local use):**
- B-C1: Remove `runner.os == 'macOS'` filter from the secrets-verification step or lift it into a dedicated pre-matrix gate job.
- B-C2: Add `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to the CI `missing=()` check, or add a doc line in `SECURITY.md` that an empty-string secret value is mandatory (not unset).
- B-C3 (optional): Fix `SECURITY.md` terminology `minisign` → `rsign2` or clarify the relationship.

## Artefact C — Phase 4c spec (`docs/specs/m2-phase4c-release-readiness-design.md`)

1. **Structural:** PASS — 8 in-scope items with a clean OOS list. Success criteria map 1:1 to scope items.
2. **Architecture:** PASS — Correctly defers Apple work while closing everything parallel. Ad-hoc signing path is the right daily-use compromise.
3. **Security:** PASS with caveat — The downgrade-protection design (§3) is correct: client-side version check + in-manifest version. Layer 2 wording ("in-manifest version string") should clarify whether this is Tauri's native updater manifest schema (which already includes `version`) or a custom addition; if it's the former, no new code is needed and the test should assert Tauri's built-in check is enabled.
4. **Completeness:** WARN — Missing a cross-cutting item: ADR-013 implies `src-tauri/capabilities/default.json` `persisted-scope` allow-list will change for Phase 4d. Phase 4c's "Keychain scope tightening audit" §7 should note that capability file is also touched by 4d, so any narrowing done in 4c should be 4d-aware. Cosmetic but avoids rework.
5. **Risk register:** PASS — Risks table covers public-repo history secrets, CDN cost, ad-hoc-sign breakage, smoke-test flakiness, semver-vs-string downgrade false-positives. The semver edge-case is a nice catch.
6. **Spec alignment:** PASS — No conflict with ADR-013. Section 5 (`local-ad-hoc-sign.sh`) and ADR-013 Phase 4d are orthogonal.
7. **Readiness:** PASS — Enough detail to execute. The Option A/B `latest.json` decision is correctly left to Moe with a defaulted recommendation (A).

**Local ad-hoc-sign daily-use safety check:** `codesign --force --deep --sign -` is the documented ad-hoc path; combined with `xattr -d com.apple.quarantine` it produces an app that launches on current macOS (Sonoma/Sequoia) without the "unverified developer" prompt **on the machine that built it**. Hidden sharp edges:
- If Moe has System Integrity Protection + "Allow apps downloaded from: App Store & identified developers" (the default), ad-hoc is accepted. If he's on "App Store only" (rare), it will fail. Worth a one-liner in the README's Local-use section.
- AirDrop or scp-ing the `.app` to another Mac re-quarantines it and ad-hoc sig won't clear it on that second machine. Fine for single-machine daily use; not a distribution path.
- `codesign --deep` is deprecated by Apple — still works today but Tauri's own signing pipeline uses per-bundle signing. Acceptable for a local script; don't copy-paste into the production signing flow.

**No hidden Gatekeeper showstopper for Moe's daily use.** Script is safe.

**Conditions:**
- C-C1: Clarify §3 Layer 2 on whether it's Tauri's native `version` manifest field or a custom manifest.
- C-C2: Add a note in §7 that any `capabilities/default.json` change coordinates with Phase 4d.
- C-C3 (suggestion): Add a README line about Gatekeeper mode sensitivity for the ad-hoc sign path.

## Consolidated conditions (must-fix before proceeding)

**Must-fix before Moe uses the app daily:** None.

**Must-fix before tagging `v0.1.0` (first real public release):**
1. B-C1: Remove `runner.os == 'macOS'` filter from CI secrets-verification or move to pre-matrix gate.
2. B-C2: Add `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to the CI missing-secrets check, or document empty-string convention explicitly.
3. Phase 4c §1 round-trip validation must actually run green before tagging.

**Should-fix during Phase 4c:**
4. C-C1: Clarify downgrade-protection Layer 2 mechanism (native manifest vs. custom).
5. C-C2: 4c §7 keychain audit notes 4d capabilities coordination.
6. B-C3: `SECURITY.md` terminology cleanup (`rsign2` vs. `minisign`).

**Cosmetic / nice-to-have:**
7. C-C3: README note on Gatekeeper mode for ad-hoc sign.

## Green-lights — what Moe can do TODAY without waiting for anything

1. **Commit ADR-013 work as-is.** Already done in `2466c6a`. Use the canonical paths from now on.
2. **Run `tools/scripts/local-ad-hoc-sign.sh`** once the script is written (Phase 4c §5). In the meantime, `codesign --force --deep --sign - <app>` + `xattr -d com.apple.quarantine` manually gets him a daily-runnable app.
3. **Use the app daily.** None of the open conditions affect local use. The updater won't function without a published `latest.json`, but he doesn't need updates yet — he's rebuilding from source when he wants changes.
4. **Start Phase 4c execution.** Items 3, 4, 5, 7, 8 (downgrade tests, DMG art, ad-hoc-sign script, keychain audit, CHANGELOG) are all fully actionable with zero Apple dependency.
5. **Make repo public (Phase 4c §2 Option A) whenever ready.** Run `gitleaks` history scan first per Phase 4c Risk #1. This unblocks the `latest.json` path for free.
6. **Upload `TAURI_SIGNING_PRIVATE_KEY` and empty-string `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to GitHub Secrets now.** The CI guard will then block cleanly on a premature tag push.
7. **Begin Phase 4d planning for vault migration** — doesn't block 4c, and 4c explicitly lists 4d as out-of-scope.
8. **Start M3 brainstorming** — fully independent of Apple and of 4c.

Items that still wait:
- Public notarized DMG distribution — pending Apple enrolment.
- External consultant installs — pending Phase 4d + Apple.

## Sign-off Decision

**PROCEED WITH CONDITIONS.**

Moe can use the app daily today. Phase 4c is cleared for execution. The two CI-guard defects in artefact B must be fixed before the first `v*` tag push but do not impact local use or Phase 4c work. Phase 4d (vault migration per ADR-013) is correctly deferred and not a blocker for daily consumption of the app by Moe himself.

Reviewed by: Codex
Date: 2026-04-17
