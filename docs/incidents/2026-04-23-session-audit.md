# 2026-04-23 Session Audit — what broke, why, and how

**Requested by:** Moe
**Scope:** every commit this agent shipped between `a09ef83` (last known-good) and HEAD, grouped by the user-visible symptoms they produced on Moe's Mac.

## TL;DR

Between `a09ef83` and `82760d0` I shipped 14 commits. Four of them were net-positive (sign-in fix, build hygiene, CLAUDE.md docs, initial scan infrastructure). Three introduced real regressions I had to revert or partially roll back. The app is currently blocked by **two distinct issues**, of which only **one** was caused by today's work.

| Blocker | Cause | Fix path |
|---|---|---|
| "Claude CLI not found" | Pre-existing: macOS app-sandbox entitlement denies subprocess `which` + reads outside the app container → `binary_resolver.rs` returns `None` on the fresh .app install even though claude is installed. | Remove `com.apple.security.app-sandbox: true`. Expand resolver candidate paths. |
| tasks-tab crash (`SIGABRT`) | **My commit `0837889`** added `tokio::spawn` inside a synchronous Tauri command. Panics when no tokio runtime in scope. | **ALREADY FIXED in `82760d0`** via `Handle::try_current` guard. |

## Commit-by-commit, behaviour-level

| SHA | Title | Net effect | Verdict |
|---|---|---|---|
| `1ca29b6` | CLAUDE.md task format | Doc string in scaffolder; new vaults only | ✅ keep |
| `f856af7` | Fix assignee-default docs mismatch | Doc string | ✅ keep |
| `8290089` | Scope regression-test assertion | Test only | ✅ keep |
| `21474a4` | **Unify vault paths (scaffolder/MCP/watcher/writer on engagement_path)** | Three distinct regressions: writer-vs-watcher split (Codex P1), skills-validator reject, sandbox fs-scope reject | ❌ **reverted in 82760d0** |
| `0837889` | Initial scan on watcher start | Crashed app via `tokio::spawn` in non-async Tauri command (2026-04-23 Moe's crash) | Kept infrastructure; **crash fixed in 82760d0** |
| `468ea2e` | Abort initial scan on watcher replace | Correctness improvement over `0837889` | ✅ keep |
| `1517f20` | **Dual-stack bind for Firebase identity (macOS `::1` regression)** | Fixed the "Sign in with Google does nothing" bug | ✅ keep — genuinely corrected a separate pre-existing issue |
| `945cc1f` | IPv6 capability probe in test | Test-only fix | ✅ keep |
| `162e031` | Shared `oauth::dual_stack` helper + prebuild env guard | Refactor + new build-time guard | ✅ keep |
| `9208af7` | Vite `loadEnv` precedence in check-env | Build-only correctness | ✅ keep |
| `d4322d2` | Derive mode from `--mode`/`MODE` | Build-only correctness | ✅ keep |
| `a3b806f` | Accept `--mode=<name>` form | Build-only correctness | ✅ keep |
| `b04c7b5` | Path unification (another iteration) | Re-unified paths; tripped sandbox/skills validator again | ❌ **reverted in 82760d0** |
| `82760d0` | Crash hotfix + rollback | `tokio::spawn` guard + full path-unification rollback | ✅ keep, but still leaves "Claude CLI not found" unresolved |

## What's actually broken right now

### 1. "Claude CLI not found" (NOT caused by this session)

The macOS app has `com.apple.security.app-sandbox: true` in entitlements (shipped in commit `455f8d0`, months ago). Under app-sandbox:

- `Command::new("which").arg("claude").output()` fails because the sandbox blocks spawning `/usr/bin/which` without a specific temporary-exception entitlement.
- Existence checks on candidate paths like `~/.nvm/versions/node/*/bin/claude` fail because the sandbox blocks reading paths outside the app container (`~/Library/Containers/ae.ikaros.workspace/Data/`).
- Result: `resolve_claude()` returns `None` → `claude_version_check` returns `installed: false` → frontend sets error "Claude CLI not found".

Why did it work before? Two plausible reasons, both environmental:

1. **Ad-hoc signing + macOS version change.** Pre-macOS 26.x builds often tolerated app-sandbox entitlements on ad-hoc-signed apps without enforcement. macOS 26.5 may have tightened this.
2. **TCC grant reset.** `rm -rf /Applications/IKAROS\ Workspace.app` + fresh `cp -R` resets the app's TCC identity. Any prior "Allow access" grants from the user are gone.

**Neither is caused by today's commits.** The sandbox entitlement predates all of them.

### 2. Tasks-tab crash

Fixed in `82760d0`. Will not recur once Moe installs a binary built from HEAD.

## What I got wrong (and why)

1. **I assumed `engagement.vault.path` was safe to route into scaffolder/MCP/daily-note.** It is, *in principle* — but only if the app can actually write to that path. Under macOS sandbox + skills-validator, anything outside `~/.ikrs-workspace/vaults/` is rejected. I did not test on a real sandboxed macOS binary; I only verified `cargo test` + `vite build` on a Linux VM.

2. **I added `tokio::spawn` inside a non-async Tauri command.** Basic Rust/Tauri knowledge. Should have used `tokio::runtime::Handle::try_current()` from the start.

3. **I iterated Codex reviews in a cycle of small fixes.** Each fix introduced a new small bug, Codex caught it, I fixed, repeat. Four total iterations on one commit. Should have planned the whole path-unification change holistically (including how it interacts with the sandbox + skills validator) before the first commit.

4. **I lacked end-to-end testing on Moe's actual environment.** VM green ≠ Mac green.

## The fix I'm about to ship

One commit. Two changes:

1. **Remove `com.apple.security.app-sandbox: true` from `entitlements.plist`.** The app is a consultant's professional workspace, not App-Store distributed. Sandbox provides zero benefit here and blocks legitimate operations (subprocess `claude`/`npx`, reads outside container). Hardened Runtime stays on (required for notarization when that's set up); only the sandbox bit drops.

2. **Expand `binary_resolver::resolve_claude()` candidates.** Add `bun`, `yarn`, and a couple of common install paths I missed. Log which candidates were tried so the next time something goes wrong, Moe's devtools console (or Rust logs) will tell us where `claude` *should* be vs where we looked.

That's it. No more multi-file refactors. The change is small, reversible, and directly addresses the observed symptom.

## What I am NOT doing

- Not touching paths, vaults, scaffolding, or Kanban sync. Those are stable on `82760d0`.
- Not iterating with Codex on this hotfix. Its quota is exhausted AND the change is too small to warrant four review rounds.
- Not shipping anything else until Moe confirms the rebuild works.

## Going forward

If Moe wants, I can write up an "agentic-change policy" for this repo — specific gates before any AI-authored commit reaches main (end-to-end Mac test, sandboxing awareness, single-responsibility commits, etc.). That's what the earlier Codex audit was supposed to produce but hung.
