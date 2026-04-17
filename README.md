# IKAROS Workspace

Desktop app for IKAROS consultants working against external clients. Embeds Claude Code, connects Gmail / Calendar / Drive / Obsidian through MCP servers per engagement, tracks time + productivity, gives line managers visibility. Tauri + React + TypeScript; runs on macOS (primary), Windows / Linux (secondary).

Status: **M1 complete, M2 Phase 4b complete, Phase 4c (release readiness) in progress.** Daily use via ad-hoc signing is supported now; distributable signed DMG is blocked on Apple Developer enrolment.

## Daily Use (No Apple Developer Cert Required)

For using the app on your own Mac while Apple enrolment processes:

```bash
# One-time setup
npm ci

# Build + ad-hoc sign + install into /Applications
./tools/scripts/local-ad-hoc-sign.sh install
```

First launch from `/Applications`: right-click (or ctrl-click) the app → **Open** → click **Open** in the dialog. Subsequent launches: double-click as normal. This is Gatekeeper's standard override for ad-hoc-signed local builds.

The script also supports build-only (no install):
```bash
./tools/scripts/local-ad-hoc-sign.sh
```

**What ad-hoc signing means:** the `.app` bundle is signed with identity `-` (self-signed, local-only). Works on your Mac. Cannot be distributed to other users — Gatekeeper on their machine will reject it. Distribution requires a real Apple Developer ID cert + notarization; see `SECURITY.md`.

## Local Development

```bash
# Hot-reload dev mode (best for active development)
npx tauri dev

# Production build (unsigned .app — run via the ad-hoc sign script above
# if you want to test the installed-app flow without dev mode)
npx tauri build

# Tests
npx vitest run                                    # JS / React tests
cargo test --manifest-path src-tauri/Cargo.toml   # Rust tests
```

## Architecture

- **Frontend:** React 19 + TypeScript + TailwindCSS + shadcn/ui
- **Backend:** Rust (Tauri 2), OS keychain via `keyring` crate, OAuth token refresh module
- **Embedded AI:** Claude Code CLI spawned as subprocess per engagement, with `--mcp-config` pointing at an auto-generated `.mcp-config.json`
- **MCP servers per engagement:** Gmail, Google Calendar, Google Drive, Obsidian
- **Auth:** IKAROS identity (Firebase) + per-engagement Google OAuth (PKCE) for client accounts
- **Storage:** Firestore for metadata + cross-device sync; OS keychain for tokens; local filesystem for Obsidian vaults

See `docs/specs/` for full design. `embedded-claude-architecture.md` is the parent M2 spec; phase specs are `m2-phase{1,2,3a,3b,3c,4a,4b,4c}-*.md`.

## Repositories

- This app: `IKAROSgit/ikrs-workspace` (currently private; Phase 4c decides public-vs-private + CDN for `latest.json`)
- Parent monorepo: `IKAROSgit/ikaros-platform` — the OpenClaw agent system, where this app's architecture decisions are recorded (see `.architecture/DECISIONS.md` ADR-013 for Obsidian vault paths)

## Security Posture

See `SECURITY.md` for secret inventory, updater key management, rotation procedure, and incident response.

Short version:
- Updater signing key generated with Tauri's `signer generate` (Ed25519, rsign2 format), key ID `2363FF5A3F800DC9`. Public key committed at `src-tauri/tauri.conf.json:29`. Private key lives at `~/.tauri/ikrs-workspace.key` outside the repo; must be uploaded to GitHub Secrets before any `v*` tag push.
- CI rejects tag pushes that still contain a placeholder pubkey or are missing required signing secrets (enforced on every matrix OS).
- Per-engagement Google OAuth uses PKCE (desktop-app public-client pattern). Tokens go to the OS keychain only.

## Governance

Every phase must be Codex-reviewed before merging — Golden Rule #11 in the parent monorepo's `CLAUDE.md`. Reviews live in `.output/codex-reviews/`. No review → no merge.

## Contributing

Single maintainer (Moe Aqeel, CEO of IKAROS, `moe@ikaros.ae`). External contributions not accepted at this stage.
