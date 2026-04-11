# Codex Tier 2 Phase Review -- M2 Phase 2: Skill System

**Reviewer:** Codex (claude-opus-4-6)
**Date:** 2026-04-11
**Commit:** `e5101a0` (12 files, +1643 -25)
**Plan:** `docs/superpowers/plans/2026-04-11-m2-phase2-skill-system.md`
**Spec:** `docs/specs/embedded-claude-architecture.md` (sections 3.6-3.8)

## Verdict: PASS (8/10)

All 4 Codex conditions from plan review are fixed. All builds and tests pass.
Two Important issues identified (not blockers, but should be addressed).

---

## 1. Structural Validation

**Build results:**
- `cargo check`: PASS (13 pre-existing warnings, 0 new)
- `cargo test`: 20/20 PASS
- `tsc --noEmit`: PASS (clean)
- `vitest run`: 34/34 PASS (5 test files)
- `npm run build`: PASS (vite build clean)

**Module structure:**
- `src-tauri/src/skills/` -- 5 files (mod.rs, templates.rs, scaffold.rs, sync.rs, commands.rs)
- `src/types/skills.ts` -- TypeScript mirror types
- `src/components/skills/SkillStatusPanel.tsx` -- UI component
- `src/lib/tauri-commands.ts` -- 3 new IPC bindings added
- `src/views/SettingsView.tsx` -- Integration point modified
- `tests/unit/skills-types.test.ts` -- Type guard tests
- `src/types/index.ts` -- Re-exports added
- `src-tauri/src/lib.rs` -- 3 commands registered

All files match the plan's "Files to CREATE" and "Files to MODIFY" tables exactly. No extra files, no missing files.

## 2. Architecture -- Spec Alignment

**Section 3.6 (Orchestrator CLAUDE.md):** MATCH. The `ORCHESTRATOR_TEMPLATE` in `templates.rs` is character-for-character identical to spec section 3.6. All 8 quality gates present. All 8 domain folder references present. Variable interpolation matches `{braces}` spec. Tests verify this (`test_orchestrator_has_all_8_quality_gates`, `test_orchestrator_references_all_8_domains`).

**Section 3.7 (Domain Templates):** MATCH. All 8 domain templates match their spec counterparts verbatim. Note: spec header says "7 bundled templates" but lists 8 -- this is a spec typo (the count of 8 in the plan and implementation is correct).

**Section 3.8 (Sync & Evolution):** PARTIAL MATCH -- see Important issue I1 below. The `.skill-version` JSON schema matches. The check algorithm matches steps 1-3a. However:
- Spec step 3b says "For un-customized folders: update CLAUDE.md **silently**" (auto-update on engagement open)
- Implementation requires explicit user action via `applySkillUpdates` button
- This is arguably a UX improvement (safer), but it deviates from spec

**Subfolder structure:** Matches spec's folder tree diagram. All subfolders per domain match (communications: meetings/drafts/final/templates, etc.).

**Template interpolation:** Simple `{braces}` string replacement as specified. 8 variables: client_name, client_slug, engagement_title, engagement_description, consultant_name, consultant_email, timezone, start_date. All replaced correctly.

## 3. Security -- Path Traversal (C1)

**FIXED.** `validate_engagement_path()` implemented in both `scaffold.rs` and `sync.rs`.

- Uses `canonicalize()` to resolve symlinks before checking prefix
- Checks `starts_with(~/.ikrs-workspace/vaults/)`
- Applied to ALL 3 commands: scaffold, check, apply
- 3 tests verify rejection: `test_path_traversal_rejected`, `test_path_traversal_rejected_check`, `test_path_traversal_rejected_apply`

**Important issue I2:** The `validate_engagement_path` function is duplicated between `scaffold.rs` and `sync.rs` (identical code). This should be extracted to a shared location to prevent drift. See I2 below.

**Canonicalize fallback:** When `canonicalize()` fails (path doesn't exist yet), it falls back to the raw path (`unwrap_or(p.clone())`). For scaffold, this is acceptable because the path may not exist yet and `create_dir_all` creates it after validation. The `create_dir_all(&allowed_base)` ensures the base directory exists so prefix checking still works. For an attacker-provided path like `/tmp/evil`, canonicalize succeeds and correctly rejects it. For a new engagement path under vaults/, the path won't exist yet but its prefix still starts with the allowed base. This is sound.

## 4. Completeness

| Requirement | Status |
|-------------|--------|
| 8 domain templates | PRESENT (communications, planning, creative, operations, legal, finance, research, talent) |
| Orchestrator template | PRESENT with all 8 quality gates |
| Variable interpolation | PRESENT (8 variables) |
| .skill-version tracking | PRESENT (JSON with template_version, scaffolded_at, customized_folders) |
| Sync detection | PRESENT (compares content hash, not just version) |
| Customization protection | PRESENT (customized folders skipped in updates) |
| Idempotent scaffolding | PRESENT (test_scaffold_is_idempotent verifies) |
| Path traversal protection | PRESENT on all 3 commands |
| Unknown domain rejection | PRESENT (apply_skill_updates rejects unknown domains) |
| SkillStatusPanel UI | PRESENT with domain grid, badges, update button |
| Tauri IPC commands | PRESENT (3 commands: scaffold, check, apply) |
| spawn_blocking for FS ops | PRESENT (all 3 commands use tokio::task::spawn_blocking) |

## 5. Risk Register

**New risks from this phase:**

| Risk | Severity | Mitigation |
|------|----------|------------|
| Hardcoded `~/.ikrs-workspace/vaults/` path conflicts with spec's user-selected workspace root (D3) | Low | Pre-existing from M1 vault.rs. Spec's user-selected path is a Phase 4 (Distribution) concern. Acceptable for now. |
| `validate_engagement_path` duplication may drift | Medium | Extract to shared function (see I2). |
| No orchestrator sync -- spec says orchestrator should also be syncable | Low | Comment in `apply_skill_updates` acknowledges this as future work. Acceptable. |

## 6. Codex Conditions -- Verification

**C1 (Path traversal protection): FIXED.**
- `validate_engagement_path()` in both scaffold.rs and sync.rs
- Called on every command entry point
- 3 dedicated tests

**C2 (Creative arrow): FIXED.**
- `templates.rs` line 167: `Presentation decks (markdown outline → content)` uses U+2192
- Test `test_creative_uses_arrow_not_em_dash` verifies both presence of arrow and absence of em dash in that phrase

**C3 (Semver comparison): FIXED.**
- `is_newer_version()` in sync.rs parses major.minor.patch as tuple, compares with `>`
- `test_semver_comparison` covers: newer, equal, older, cross-component comparisons (6 assertions)
- No string equality (`!=`) used for version comparison

**C4 (useMemo for skillUpdateParams): FIXED.**
- `SettingsView.tsx` line 41: `const skillUpdateParams: SkillUpdateParams | null = useMemo(() => { ... }, [activeEngagementId, consultant, engagements, clients]);`
- Proper dependency array prevents infinite re-render

## 7. Readiness for Phase 3

Phase 3 (Polished UX + MCP) can proceed. Prerequisites met:
- Skill system fully operational
- Engagement folder structure scaffolded on creation
- Skill status visible in UI
- IPC layer complete

**Missing for Phase 3:**
- `ToolActivityCard.tsx` -- new component needed
- `SessionIndicator.tsx` -- new component needed
- Session resume (`--resume {session_id}`) -- new session management
- Per-engagement MCP config wiring
- ChatView polish (streaming UX, cost display)

**No blockers from Phase 2.**

---

## Issues

### I1 (Important): Spec deviation in sync behavior

**Spec 3.8 step 3b:** "For un-customized folders: update CLAUDE.md silently"
**Implementation:** Shows update badges and requires user to click "Update X skills" button.

The implementation is arguably safer (explicit user consent before overwriting files), but it deviates from the spec's auto-update model. If this is intentional, update spec section 3.8 step 3b to say "For un-customized folders: show update available badge and allow user to apply."

**Recommendation:** Update spec to match implementation (the implementation's UX is better).

### I2 (Important): Duplicated `validate_engagement_path`

The function is copy-pasted identically in `scaffold.rs:32-44` and `sync.rs:27-39`. If one is updated (e.g., to change the allowed base path for user-selected workspace root in Phase 4), the other must be updated manually.

**Recommendation:** Move to a shared location, either in `mod.rs` or a new `validation.rs`, and import from both modules. This is a 5-minute refactor.

### S1 (Suggestion): Unused variables in commands.rs

`commands.rs` lines 64 and 99: `let path = engagement_path.clone();` and `let folders = folders_to_update.clone();` -- the `engagement_path` and `folders_to_update` are already owned values (received by-value from Tauri IPC). The clones are unnecessary since the originals are not used after the spawn_blocking closure captures them. However, Rust's move semantics require the closure to take ownership, and the clones ensure the original binding isn't consumed. This is technically correct but verbose. Using `move` closure directly with the original bindings would be cleaner.

### S2 (Suggestion): Spec typo

Spec section 3.7 header says "These are the 7 bundled templates" but lists 8 (communications, planning, creative, operations, legal, finance, research, talent). Should say "8 bundled templates."

---

**Codex verdict: PASS (8/10)**

Conditions: I1 and I2 should be addressed in the same milestone (before M2 complete). Neither blocks Phase 3.
