# M2 Phase 4d: Canonical Vault Migration (per ADR-013)

**Status:** Draft — awaits Codex spec sign-off, then Moe's Mac presence for content move.
**Date:** 2026-04-17 (amended 2026-04-17 for M3 scope-lock pivot; see "Reviewer pivot" below)
**Parent ADR:** `ikaros-platform:.architecture/DECISIONS.md` ADR-013 (Obsidian Canonical Paths)
**Prior phases:** 4a (sandbox + signing), 4b (distribution polish), 4c (release readiness)
**Dependency:** Moe's Mac — Google Drive client signed in and `Shared drives/99 Agent Drive/Claude - IKRS/Obsidian Vault/` accessible as a filesystem path.

---

## Reviewer pivot (2026-04-17 amendment)

The original M1 and 4d thinking assumed vault readers were **internal** IKAROS line managers — i.e. IKAROS employees with access to the same Shared Drive by org membership. The M3 scope lock (`docs/planning/2026-04-17-m3-scope-lock.md`) reframed the reviewer as the **external client** (e.g. someone at `@blr-world.com`, not `@ikaros.ae`). Different principal, different NDA posture, different ACL model.

Implications preserved in this phase:
- The Drive-path decision (ADR-013) is unchanged — consultant vaults still land at `~/Library/CloudStorage/…/Claude - IKRS/Obsidian Vault/engagements/{client-slug}/` on the consultant's Mac, and still sync to the Shared Drive.
- The **visibility mechanism** (who sees what) shifts from "grant a role in the Shared Drive to IKAROS folks" to "grant a per-engagement Drive ACL to specific external client email addresses." This is **not** implemented in 4d — it's an M3/M4 concern.
- 4d still ships as planned (path resolver, migration CLI, capability scope) because the path move happens regardless of who eventually reads the content.
- What 4d **does not** promise anymore: Drive ACL provisioning. That moves into the M3+ track as "client portal" + vault access plumbing.

---

## Goal

Move consultant engagement vaults from the M1 default (`~/.ikrs-workspace/vaults/{client-slug}/`) to the ADR-013 canonical path (`~/Library/CloudStorage/GoogleDrive-moe@ikaros.ae/Shared drives/99 Agent Drive/Claude - IKRS/Obsidian Vault/engagements/{client-slug}/`). Drive-syncs engagement notes, enables later visibility to both internal reviewers (Shared Drive membership) and external clients (per-engagement ACLs granted in M3+), removes the "four parallel Obsidian set-ups" problem on Moe's Mac. Must complete before any external consultant installs the app, otherwise new installs inherit the deprecated path as their default.

## Scope

### In scope

1. **Rust path resolver** — new `src-tauri/src/vault_paths.rs` with `resolve_canonical_vault_root()` that consults an env override (`IKRS_VAULT_ROOT`) first, falls back to a per-OS default. macOS default is the full Drive path; Linux/Windows keep `~/.ikrs-workspace/vaults/` (consultant market is Mac-only for now, Linux/Windows are dev-only targets).
2. **Spec amendments** — update M1 design spec line 83+, Phase 3b spec lines 83/129, Phase 4a spec line 303 to reference the resolver instead of hard-coding `~/.ikrs-workspace/vaults/`.
3. **Rust code updates** — `src-tauri/src/commands/vault.rs`, `src-tauri/src/skills/*` call the resolver instead of using hard-coded paths. Path-traversal checks still use resolved path as their base.
4. **Frontend updates** — `src/views/SettingsView.tsx` reads the resolved path via a new `get_vault_root_path` Tauri command (no need to embed the string in JS).
5. **Tauri capability scope** — `src-tauri/capabilities/default.json` `persisted-scope` allow-list narrowed to the new path, with a transitional allow for the old path until migration is verified.
6. **Migration CLI** — `tools/scripts/migrate-vaults.sh` — scans `~/.ikrs-workspace/vaults/*/` on the user's Mac, moves each to the canonical Drive path preserving timestamps, updates the corresponding engagement records in Firestore to reflect the new path, leaves a symlink behind for safety rollback.
7. **Firestore migration script** — Node script (`tools/scripts/update-engagement-paths.mjs`) that reads all of a consultant's engagements and updates their `vaultPath` field to the new format. Dry-run flag required; writes a rollback log.
8. **Tests** — `tests/unit/lib/vault-paths.test.ts` + `src-tauri/src/vault_paths.rs` unit tests covering: env override present/absent, OS detection, path-with-spaces tolerance, path traversal rejection against the new base.

### Out of scope

1. The actual content move on Moe's Mac — a manual command run by him (or run by him under agent supervision) after this phase ships.
2. Line-manager ACL provisioning in Drive — a separate Drive admin task; product scope adjusts the app to work with whatever ACLs the admin sets.
3. Multi-consultant isolation (each consultant seeing only their own engagements' vaults) — M3.
4. Obsidian application auto-launch with the new vault — Obsidian handles vault discovery on its own.
5. Conflict resolution for Drive-vs-local concurrent writes — single-user semantics assumed; M3 handles multi-user.

---

## Design

### 1. Path Resolver (Rust)

```rust
// src-tauri/src/vault_paths.rs
use std::path::PathBuf;

pub const ENV_OVERRIDE: &str = "IKRS_VAULT_ROOT";

/// Resolves the canonical vault root for the running platform,
/// honouring an env override if set. Returns an absolute PathBuf.
/// On failure, falls back to `~/.ikrs-workspace/vaults` (the M1 path).
pub fn resolve_canonical_vault_root() -> PathBuf {
    if let Ok(v) = std::env::var(ENV_OVERRIDE) {
        if !v.is_empty() { return PathBuf::from(v); }
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));

    #[cfg(target_os = "macos")]
    {
        // ADR-013 canonical path.
        home.join("Library")
            .join("CloudStorage")
            .join("GoogleDrive-moe@ikaros.ae")
            .join("Shared drives")
            .join("99 Agent Drive")
            .join("Claude - IKRS")
            .join("Obsidian Vault")
            .join("engagements")
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Linux/Windows remain on the M1 path — consultant market is Mac.
        home.join(".ikrs-workspace").join("vaults")
    }
}
```

Every caller that previously built `~/.ikrs-workspace/vaults/{slug}` now does:

```rust
let vault = resolve_canonical_vault_root().join(&engagement.client_slug);
```

### 2. Frontend Command

```rust
// src-tauri/src/commands/vault.rs
#[tauri::command]
pub fn get_vault_root_path() -> Result<String, String> {
    Ok(resolve_canonical_vault_root().to_string_lossy().into_owned())
}
```

Registered in `lib.rs` invoke handler. `SettingsView.tsx` uses `invoke<string>('get_vault_root_path')` to display the root, removing the hard-coded string currently at line 74.

### 3. Capability Scope

```json
// src-tauri/capabilities/default.json (excerpt)
{
  "identifier": "persisted-scope",
  "allow": [
    { "path": "$HOME/Library/CloudStorage/GoogleDrive-moe@ikaros.ae/Shared drives/99 Agent Drive/Claude - IKRS/Obsidian Vault/engagements/**" },
    // Transitional — drops in the release AFTER migration is fully verified:
    { "path": "$HOME/.ikrs-workspace/vaults/**" }
  ]
}
```

macOS sandbox tolerates paths with spaces and ampersands when properly quoted; a spec risk item tracks the Gatekeeper validation.

### 4. Migration CLI

```bash
#!/usr/bin/env bash
# tools/scripts/migrate-vaults.sh — Moves ~/.ikrs-workspace/vaults/*
# into the canonical Drive path. Idempotent, preserves timestamps,
# leaves symlinks for safety rollback. Dry-run by default.
# ...
```

Invocation:
```
./tools/scripts/migrate-vaults.sh --dry-run       # default; prints the plan
./tools/scripts/migrate-vaults.sh --apply         # does the move
./tools/scripts/migrate-vaults.sh --rollback      # removes symlinks, restores from Drive back to local if interrupted
```

### 5. Firestore Engagement Records

Every `engagement` document in Firestore has a `vaultPath` field set at creation time. The Node migration script iterates `collectionGroup('engagements')` scoped to the authenticated consultant, rewrites the field if it points to the old location.

### 6. Test Fixtures

- `vault-paths.test.ts` mocks `std::env::var` and `dirs::home_dir` via the Rust test harness; covers: env override set, env override empty, env override unset on macOS, env override unset on Linux, path traversal rejection.
- Frontend tests mock `invoke('get_vault_root_path')` and assert SettingsView renders the resolved path.

---

## Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| macOS sandbox rejects the long path with spaces | Vault reads/writes fail silently under sandbox enforcement | Empirical test under an ad-hoc-signed build before wider rollout. Fallback: keep transitional allow for old path for one release after migration. |
| Google Drive client not signed in on consultant's Mac | Path resolves to a non-existent directory; app errors on first vault read | First-launch check: verify path exists, prompt consultant to sign into Drive. Graceful degradation: if Drive unavailable, offer local-only fallback. |
| GDrive sync delay causes "newly-saved note not visible yet" on a second device | Consultant thinks their save was lost | Document. Obsidian handles this natively; UX hint in status bar. |
| Simultaneous edits from external client reviewer + consultant produce sync conflicts | Drive creates duplicate-conflict files | M3 client access is read-only per scope lock. M4 handles any bidirectional concurrency. |
| Migration script leaves the source directory in a partial state on interrupt | Data loss | Dry-run mode + transactional move (copy → verify → delete source). Symlink fallback for safety rollback. |
| Phase 4a persisted-scope plugin cached the old path | After migration, the plugin may still "remember" the old scope | Clear the plugin's state file as part of migration; re-register on next launch. |
| Firestore migration script runs against wrong project | Data corruption | Require explicit `--project=ikaros-portal` flag; no default. |

## Success Criteria

1. `resolve_canonical_vault_root()` returns the correct path on macOS + Linux; unit tests cover env override, absence of override, path traversal.
2. Every Rust call site previously using `~/.ikrs-workspace/vaults/` now calls the resolver.
3. `SettingsView` renders the resolved path via the new Tauri command (no hard-coded string in JS).
4. Migration CLI dry-run accurately describes the move; `--apply` completes without data loss; `--rollback` reverts cleanly.
5. Firestore migration script updates `vaultPath` for all engagements owned by the authenticated consultant.
6. `persisted-scope` allow-list includes the new path; sandbox does not deny vault reads under an ad-hoc-signed build on Moe's Mac.
7. At least one end-to-end test: create engagement, write a note, close app, reopen, note still present (prove sync + reload).
8. Codex final-checkpoint review: PASS 9+/10.

## Codex Checkpoints

- **Ck-1:** After resolver + spec amendments land, before code migration. Reviews architectural soundness.
- **Ck-2:** After code migration + tests, before first on-Mac dry run. Reviews test coverage + risk closure.
- **Ck-3:** After Moe runs the live migration on his Mac. Reviews operational success + any deviations.
- **Final:** End of phase. Reviews readiness for external consultant distribution (i.e. Phase 4d is done when a brand-new Mac install would land on the correct path with no legacy fallback).

## What Needs Moe Action

1. **Choose migration window** — the actual move is one command but it moves your live engagement notes. Pick a time you're not mid-session.
2. **Confirm Drive path is mountable** — on your Mac, verify `ls '~/Library/CloudStorage/GoogleDrive-moe@ikaros.ae/Shared drives/99 Agent Drive/Claude - IKRS/'` returns content. If not, sign into the GDrive desktop client and re-check.
3. **Run `migrate-vaults.sh --dry-run`** first, review output, then `--apply`.
4. **Run `update-engagement-paths.mjs --project=ikaros-portal`** to align Firestore.
5. **Restart the app** and verify `SettingsView` shows the new path and the Notes view still reads your engagement content.

Phase 4d ships as code + scripts in this repo; the actual migration is a brief operational step you execute when ready.
