# Implementation Plan — M2 Phase 4c: Release Readiness (Non-Apple-Blocked)

**Spec:** `docs/specs/m2-phase4c-release-readiness-design.md`
**Codex sign-off on spec:** 2026-04-17 bundle review — PASS WITH CONDITIONS 8.5/10 (conditions addressed in-spec before this plan was written)
**Status:** In progress (started 2026-04-17)
**Goal:** Close every Phase 4a/4b ship-blocker that does not depend on Apple Developer enrolment, so `v0.1.0` tags cleanly the moment the cert arrives.

---

## Waves

### Wave 1 — Daily-use enablement (SHIPPED 2026-04-17 morning)

| Task | File(s) | Commit | Status |
|------|---------|--------|--------|
| 1. Updater keypair generated + pubkey committed | `src-tauri/tauri.conf.json:29`, `~/.tauri/ikrs-workspace.key*` | `a00c46d` | Done |
| 2. SECURITY.md with secret inventory + rotation | `SECURITY.md` | `a00c46d` | Done |
| 3. CI guard against placeholder pubkey + missing secrets | `.github/workflows/ci.yml` | `a00c46d` + Wave 2 below fixes two defects Codex flagged | Done (wave-2 amended) |
| 4. Local ad-hoc sign + install script | `tools/scripts/local-ad-hoc-sign.sh` | Wave 2 below | Done |
| 5. README replaces Tauri boilerplate with product overview + daily-use instructions | `README.md` | Wave 2 below | Done |
| 6. CHANGELOG.md covering M1 + M2 Phases 1 through 4c | `CHANGELOG.md` | Wave 2 below | Done |

**Wave 1 outcome:** Moe can build + install + launch IKAROS Workspace on his Mac daily. Next ad-hoc install is one command: `./tools/scripts/local-ad-hoc-sign.sh install`.

### Wave 2 — Codex-condition closure + governance polish (SHIPPING 2026-04-17 midday)

| Task | File(s) | Codex origin | Status |
|------|---------|--------------|--------|
| 7. Fix CI guard os-filter bug (run on every matrix OS, not only macOS) | `.github/workflows/ci.yml` | Codex 2026-04-17 bundle review B-C1 | Done |
| 8. Add `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` presence check to CI guard | `.github/workflows/ci.yml` | B-C2 | Done |
| 9. Correct SECURITY.md "minisign" naming → Tauri rsign2 format | `SECURITY.md` | B-C3 | Done |
| 10. Amend Phase 4c spec §3 Layer 2 to clarify it uses Tauri's native `version` field | `docs/specs/m2-phase4c-release-readiness-design.md` | C-1 | Done |
| 11. Amend Phase 4c spec §7 to coordinate with Phase 4d capability rewrite | `docs/specs/m2-phase4c-release-readiness-design.md` | C-2 | Done |
| 12. Commit Wave 1 + Wave 2 as "Phase 4c scaffolding" | — | — | Next |

### Wave 3 — Feature work (NEXT)

| Task | File(s) | Spec ref | Status |
|------|---------|----------|--------|
| 13. `src/lib/version-compare.ts` — semver-ish `isNewerVersion()` helper | new | Phase 4c §3 Layer 2 | Pending |
| 14. `UpdateChecker` — insert Layer 2 check before `downloadAndInstall()` | `src/components/UpdateChecker.tsx` | §3 | Pending |
| 15. `tests/unit/lib/version-compare.test.ts` — ≥8 cases (equal, higher, lower major/minor/patch, missing fields, pre-release, `v`-prefix, malformed) | new | §3 success criteria | Pending |
| 16. `tests/unit/components/UpdateChecker.test.tsx` — expand to cover Layer 2 rejection of same + lower version | existing | §3 | Pending |
| 17. `docs/decisions/2026-04-17-latest-json-hosting.md` — A/B decision doc | new | §2 | Pending (recommends option A, awaits Moe's call) |
| 18. SECURITY.md § "Keychain scope audit" — enumerate every Rust read site + decision to defer narrowing | `SECURITY.md` | §7 | Pending |

### Wave 4 — CI / smoke test

| Task | File(s) | Spec ref | Status |
|------|---------|----------|--------|
| 19. `.github/workflows/smoke-test.yml` — clean-machine install smoke test on macos-latest runner | new | §6 | Pending |
| 20. Mock harness for Claude CLI + MCP servers used by the smoke test | `tests/smoke/` | §6 | Pending (minimal scope — a stub `claude` that returns a canned `system.init` stream) |

### Wave 5 — Finishing + Codex review

| Task | File(s) | Spec ref | Status |
|------|---------|----------|--------|
| 21. Final Phase 4c Codex checkpoint review against shipped code | `.output/codex-reviews/` | — | Pending |
| 22. Update `CHANGELOG.md` with real git sha for 4c completion | `CHANGELOG.md` | — | Pending |
| 23. Session handoff `2026-04-17-m2-phase4c-session-handoff.md` | `.output/` | GR-12 | Pending |

### Out of scope (deferred or user-blocked)

| Deferred item | Why | Next step |
|---------------|-----|-----------|
| `TAURI_SIGNING_PRIVATE_KEY` upload to GH Secrets | Requires Moe to paste `~/.tauri/ikrs-workspace.key` contents into repo Settings → Secrets | Moe does this manually; no code change |
| Updater round-trip validation test (spec §1) | Requires the GH Secret above so CI can actually sign | Wait on Moe, then run a `test-signing-*` branch push |
| `tauri.conf.json` updater endpoint change | Depends on Wave 3 hosting decision (A vs B). If A → no change; if B → new CDN URL | Wait on Moe's A/B call |
| DMG background art (spec §4) | Design work; best done on Mac with `ui-ux-pro-max` skill + actual render preview | Schedule a separate design session |
| Apple Developer enrolment | User-blocked (Apple side reviewing) | Out of this phase entirely |

---

## Codex Checkpoints (planned)

- **Ck-1 (at end of Wave 2):** `2026-04-17-m2-phase4c-ck1-scaffolding-review.md` — validates Wave 1+2 as the foundation is stable before shipping Wave 3 feature code.
- **Ck-2 (at end of Wave 3):** `2026-04-17-m2-phase4c-ck2-feature-review.md` — focused on downgrade-protection correctness and keychain audit completeness.
- **Final (at Wave 5):** `2026-04-17-m2-phase4c-final-review.md` — full 7-point with readiness verdict. Target: PASS 9+/10, all conditions closed.

## Success Criteria (mirrors spec)

Tracked as they complete:

- [x] Updater pubkey is real (not placeholder) ✔ `a00c46d`
- [ ] `TAURI_SIGNING_PRIVATE_KEY` + `_PASSWORD` set in GH Secrets (Moe action)
- [ ] Round-trip signing validation green (Wave 5 after Moe uploads)
- [ ] Downgrade protection ≥5 test cases passing (Wave 3 Task 15)
- [ ] `latest.json` endpoint resolves publicly (Wave 3 Task 17 → Moe picks A/B → Wave 4)
- [ ] `tools/scripts/local-ad-hoc-sign.sh` exists, executable, tested ✔ Wave 1
- [ ] `smoke-test.yml` workflow green (Wave 4)
- [ ] `SECURITY.md` has keychain audit section (Wave 3 Task 18)
- [ ] `CHANGELOG.md` committed ✔ Wave 1

Phase 4c completes when every box is ticked.
