# M2 Phase 4c: Release Readiness (Non-Apple-Blocked)

**Status:** Draft
**Date:** 2026-04-17
**Parent spec:** `embedded-claude-architecture.md` (Phase 4)
**Prior phases:** 4a (sandbox + signing scaffold), 4b (distribution polish)
**Blocker:** Apple Developer enrolment is pending review; this phase closes **everything else** so that the moment the cert arrives, we tag `v0.1.0` and ship.

---

## Goal

Close the remaining non-Apple-blocked ship-blockers from the retroactive Codex sign-offs of Phases 4a (8.0/10) and 4b (7.0/10), plus harden the developer loop so Moe can use the app daily via ad-hoc signing while waiting on the cert. When this phase completes: a `v*` tag push either produces a fully notarized DMG (if cert is present) or fails CI loudly with a clear message (if not).

## Scope

### In scope

1. **Updater key operational move** — private key uploaded to GitHub Secrets, CI guard already in place (2026-04-17). Verify round-trip by building a test artifact with real signature and validating it against the pubkey now in `tauri.conf.json:29`.
2. **`latest.json` hosting decision + implementation** — either (a) make `IKAROSgit/ikrs-workspace` repo public and keep `https://github.com/.../releases/latest/download/latest.json`, or (b) keep repo private and host `latest.json` on a public Cloud Storage / Cloudflare R2 bucket with signed release-artefact URLs. Updater endpoint in `tauri.conf.json:27` updated to match.
3. **Downgrade-protection tests for the updater** — unit + integration test that proves the client rejects `latest.json` pointing to a lower version than currently installed, and rejects signature-valid payloads whose embedded version manifest is older than what's running. Targets Phase 4b Codex condition C7.
4. **DMG assets** — final background art at `src-tauri/icons/dmg-background.png` matching the IKAROS brand palette (currently a placeholder; Phase 4b Codex condition C6). Installer window sizes reviewed against the actual asset.
5. **Local ad-hoc sign workflow documented + scripted** — a `tools/scripts/local-ad-hoc-sign.sh` that takes a fresh `cargo tauri build` output and produces a `.app` Moe can drop into `/Applications` on his own Mac. Documents the "right-click → Open" first-launch dance. This is the temporary daily-use path until real signing lands.
6. **Clean-machine install smoke test** — a scripted end-to-end verification (GitHub Actions workflow on a fresh macOS runner) that installs the produced DMG, launches the app, connects an OAuth test engagement, spawns a Claude session, verifies MCP tools respond. Gates `v*` tag releases. Today no such test exists — verification is ad-hoc.
7. **Keychain scope tightening audit (informational)** — inventory what reads the keychain, document why `keyring:default` is currently blanket-permissive, decide whether Phase 4c is the place to narrow it or defer to M3. Spec output is a decision doc; code change is optional.
8. **CHANGELOG.md** — first version created, covering M1 + M2 phases as "unreleased / 0.1.0" entries.

### Out of scope (explicit)

- Apple Developer enrolment completion itself (user-blocked).
- Producing a notarized DMG for public distribution (same reason).
- Phase 4d vault migration per ADR-013 (separate phase; mechanical content move needs Moe's Mac presence).
- M3 timesheet engine, manager dashboards, client-facing messaging.
- Windows + Linux installer signing (Windows EV cert is a separate commercial purchase; Linux relies on distro signing which is low-priority).

---

## Design

### 1. Updater key round-trip validation

**Evidence current pubkey is valid:**

```bash
cat ~/.tauri/ikrs-workspace.key.pub
# => dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDIzNjNGRjVBM0Y4MDBEQzkK…
```

Matches `src-tauri/tauri.conf.json:29` as of commit <this-phase-TBD>.

**Validation task:** a CI job on `push` to a `test-signing-*` branch runs `cargo tauri build` with the real `TAURI_SIGNING_PRIVATE_KEY` secret, produces a signed update bundle (`.app.tar.gz` + `.sig`), and runs `minisign -V -p <pubkey> -m <bundle>` to prove the signature validates against the committed pubkey. If validation fails, the whole flow is broken — either the secret is mis-uploaded or the pubkey in-repo is out of date.

### 2. `latest.json` hosting

Two options:

| Option | Effort | Ongoing cost | Trade-off |
|--------|--------|--------------|-----------|
| **A: Make repo public** | 30 min — flip GitHub setting, audit commits for secrets one last time | $0 | Public commits and issue tracker; anyone can clone and build from source. Acceptable for a consultant-market app where the value is in the signed binary + brand + updates, not secret source code. |
| **B: Public CDN for `latest.json` + artefacts** | 1 day — set up a GCS or Cloudflare R2 bucket, wire GitHub Actions to upload on release, update `tauri.conf.json:27` | ~$1-3/mo | Repo stays private; public surface is just the updater manifest + release binaries. |

**Recommendation:** Option A unless there's a strategic reason to keep source private. The app's moat is the product experience and the IKAROS relationship, not the code. If the team later decides to close-source specific modules, they can be extracted; open-sourcing the shell now is low-regret.

**Decision in this phase:** Moe picks A or B. Implementation follows. Default deliverable assumes A.

### 3. Downgrade protection

Phase 4b shipped the auto-updater but never verified it rejects a lower version. Implement both layers of defence:

**Layer 1 — client-side version check:** in `src/components/UpdateChecker.tsx`, before accepting an update, compare `update.version` (from `latest.json`) to `app.getVersion()`. Reject if `latest.version <= current.version` with a log line but no user-facing error.

**Layer 2 — signature manifest:** minisign signatures cover the bundle contents. Adding an in-manifest version string that we extract and compare belt-and-braces protects against MITM substitution of an old `latest.json` pointing at old signed artefacts.

**Tests:** Vitest suite `tests/unit/updater-downgrade.test.ts` with mocked `latest.json` fixtures: same version (no update), higher version (accept), lower version (reject), missing version (reject), malformed version (reject). Target ≥5 cases, all assertions on the actual `UpdateChecker` logic.

### 4. DMG assets

Phase 4b shipped `src-tauri/icons/dmg-background.png` with window geometry at `tauri.conf.json:49-52`:

```json
"dmg": {
  "background": "icons/dmg-background.png",
  "windowSize": { "width": 660, "height": 400 },
  "appPosition": { "x": 180, "y": 170 },
  "applicationFolderPosition": { "x": 480, "y": 170 }
}
```

Phase 4c produces a final asset. Brand direction: `ui-ux-pro-max` skill for visual, mirror the IKAROS website's palette. Render at `@2x` resolution for Retina.

### 5. Local ad-hoc sign workflow

Moe needs to use the app daily while waiting on Apple. Current options:

- `npm run tauri dev` — full hot-reload, ideal for active development
- `cargo tauri build` → unsigned `.app` — requires right-click Open on first launch; breaks on macOS update that re-hardens Gatekeeper

The workflow we ship as `tools/scripts/local-ad-hoc-sign.sh`:

```bash
#!/usr/bin/env bash
# Builds and ad-hoc signs IKAROS Workspace for local use only.
# Ad-hoc signatures ("-") are accepted by macOS for locally-built apps
# provided the user has NOT enabled Gatekeeper strict mode.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo tauri build
APP="src-tauri/target/release/bundle/macos/IKAROS Workspace.app"

codesign --force --deep --sign - "$APP"
xattr -d com.apple.quarantine "$APP" 2>/dev/null || true

echo "Built and ad-hoc signed: $APP"
echo "Drag into /Applications. First launch: right-click → Open."
```

Documented in `README.md` under "Local development → Daily use".

### 6. Clean-machine install smoke test

A GitHub Actions workflow `smoke-test.yml` that runs on a fresh `macos-latest` runner:

1. Download the latest unsigned DMG artefact from the current CI run
2. `hdiutil attach` the DMG
3. `cp -r /Volumes/…/IKAROS\ Workspace.app /Applications/`
4. Launch headless via `open -a` with a synthetic OAuth token fixture
5. Assert the window appears, the main process stays alive 10s, no crash in `~/Library/Logs/DiagnosticReports/`
6. Spawn a stub Claude session with a mock MCP backend, assert the session registry has one entry
7. Exit

Gates the `v*` tag pipeline. Phase 4c deliverable is green-on-green for this workflow against a real unsigned build; once Apple lands, same workflow runs against the signed DMG.

### 7. Keychain scope audit (informational)

**Current state:** `src-tauri/capabilities/default.json` grants `keyring:default` to all webviews in the app. Any JS in any view can read any keyring entry. Phase 4a left this permissive because the refresh-token flow is the only keyring consumer and narrowing earlier would have blocked development.

**Audit output (this phase):** a section in `SECURITY.md` enumerating every keyring read site in Rust (`src-tauri/src/credentials.rs`, `oauth/token_refresh.rs`, `vault.rs`) and confirming no webview JS calls keyring directly. If that holds, the finding is: narrowing is safe but low-priority until M3 introduces per-consultant isolation. Documented and deferred.

### 8. CHANGELOG.md

First version covers M1 through Phase 4c. Follows Keep-a-Changelog format. `v0.1.0` entry at top marked "Unreleased" until the tag lands.

---

## Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Making repo public exposes commit history with stale secrets | Credentials in old commits become public | Git-history secret scan (`gitleaks`, `trufflehog`) before flipping visibility; rewrite or block-list any hits. Rotate anything found. |
| CDN cost overrun if Option B chosen and downloads spike | Unexpected bill | Set budget alert on the hosting project; per-IP rate limit at CDN edge. Not a concern at current expected usage (< 100 consultants initially). |
| Local ad-hoc sign breaks on macOS updates hardening Gatekeeper | Moe can't run daily | Worst case: fall back to `npm run tauri dev`. Long case: Apple cert lands and this becomes moot. |
| Smoke test flakiness from network timing | CI false-failures | Mock all external endpoints (Firebase, OAuth, MCP servers) in the smoke test; no real network calls. |
| Downgrade test false-positives on legitimate hotfix re-releases (e.g. `v0.1.0` → `v0.1.0-hotfix.1`) | Rollback deploy blocked | Compare using semver, not string; accept hotfix suffixes. Test case in the suite. |

## Success Criteria

1. `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are set in GitHub Secrets and a test tag push produces a signed bundle that validates against the in-repo pubkey.
2. `latest.json` URL in `tauri.conf.json:27` resolves to 200 from a public user-agent. (Documented via `curl -I` output captured in `.output/`.)
3. Downgrade protection tests pass: ≥5 cases covering lower/equal/higher/missing/malformed versions, all asserting correct UpdateChecker behaviour.
4. `src-tauri/icons/dmg-background.png` is the final art; DMG window displays correctly at @1x and @2x.
5. `tools/scripts/local-ad-hoc-sign.sh` exists, is executable, and produces a runnable `.app` on Moe's Mac in one command.
6. `smoke-test.yml` workflow runs to green on `push` to `main`.
7. `SECURITY.md` has a "Keychain scope audit" section with an enumerate-and-deferred conclusion (or narrower capabilities if the audit reveals we can tighten without breaking anything).
8. `CHANGELOG.md` exists at repo root with `v0.1.0` (Unreleased) entry covering M1 through Phase 4c.
9. Total delta: no commits touch Apple-signing-specific config except documentation updates. Everything shipped here runs without an Apple Developer cert.

## Codex Checkpoints

- C1 — After Option A/B decision committed, review blast-radius of the visibility flip (if A) or CDN wiring (if B).
- C2 — After downgrade tests land, review test coverage vs. attack surface.
- C3 — After smoke test workflow runs green, review end-to-end coverage against real user install flow.
- Final — Phase-complete review before Apple enrolment resumes.

## Exit Criteria → Next Phase

When this phase completes:
- Releasing `v0.1.0` becomes a one-step operation gated only by Apple Developer enrolment.
- Moe uses the app daily via `local-ad-hoc-sign.sh`.
- The team can move to **Phase 4d** (vault migration per ADR-013, requires Moe's Mac presence) and/or **M3** (timesheet engine + manager dashboards, independent of Apple).
