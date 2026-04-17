# Codex Review — M2 Phase 3b (Final / Checkpoint 2)

> Retroactive audit written 2026-04-16. Phase 3b shipped on 2026-04-12;
> the Checkpoint 2 review artifact was never written at the time.
> This review evaluates the code as it stood at the end of Phase 3b
> (commits a3966fb → 66dfb10) and cross-checks the current `main`
> (HEAD b4bb80a) to confirm no 3b-era regressions were introduced by
> subsequent phases.

```
CODEX REVIEW
============
Subject: M2 Phase 3b — MCP Wiring + Token Resilience (Checkpoint 2, Final)
Type: phase-review
Date: 2026-04-16
Reviewed by: Codex (via superpowers:code-reviewer per CODEX.md §"How to Request a Codex Review")

VERDICTS
--------
1. Structural:     PASS  — DAG is clean. `mcp_config.rs` isolated to Layer claude/. `McpProcessManager` fully retired: `src-tauri/src/mcp/` directory no longer exists, `pub mod mcp;` gone from `commands/mod.rs`, `lib.rs` no longer registers the 5 MCP commands or `.manage(McpProcessManager::new())`. Frontend mirror: `useEngagement.ts` deleted, wrappers `spawnMcp|killMcp|killAllMcp|mcpHealth|restartMcp` have zero references (verified via grep with word-boundaries).
2. Architecture:  PASS  — Option A honored: Claude CLI owns MCP lifecycle via `--mcp-config`. `session_manager.spawn()` signature extended with `env_vars` + `mcp_config_path` exactly as specified. Token flows through env (`${GOOGLE_ACCESS_TOKEN}`), never persisted to disk. Orchestration (`commands.rs::spawn_claude_session`) runs the 5-step sequence from Section 3 of the spec.
3. Security:       PASS  — No secrets on disk: `.mcp-config.json` contains only the `${GOOGLE_ACCESS_TOKEN}` placeholder; the actual token is injected via `Command::envs()` into the Claude CLI child only. `.mcp-config.json` is in `.gitignore` (added in 66dfb10). Keychain access uses `KeyringExt` with service `"ikrs-workspace"` and `ikrs:{engagement_id}:google` key — same pattern as `credentials.rs`. Claude subprocess retains existing `--disallowed-tools Bash` guard.
4. Completeness:   PASS — All 10 plan tasks shipped with commits: Task 1 (a3966fb/mcp_config.rs), Task 2 (a3966fb/spawn ext), Task 3 (a3966fb/orchestration), Task 4 (72877d0/manager retire), Task 5 (febdec8/frontend retire), Task 6 (14bdc71/clientSlug), Task 7 (c9d223f/McpAuthErrorPayload), Task 8 (f8118e2/stream_parser detection), Task 9 (a6e889f/store+listener), Task 10 (aa2a433/toast+re-auth), Wave 4 doc (d1442ef + gitignore 66dfb10 + Checkpoint 1 fix 6124d78). No TBDs or placeholder code.
5. Risk register:  PASS — All 6 declared risks tracked; npx cold-start mitigated downstream in Phase 4a via binary_resolver (5580ed2). Token expiry mitigation implemented (re-auth toast). One risk upgraded post-ship: stream-parser auth detection was initially keyed on `tool_id` (opaque `toolu_…`) which would have always returned `"unknown"`; Phase 3c commit 26dbb71 (Codex C2) re-keyed it to `tool_name_map` before shipping to main. This is a latent 3b bug that was caught and fixed; documenting for the register.
6. Spec alignment: PASS — Parent spec Q3 amended from "scaffold time" → "session spawn time" (d1442ef, docs/specs/embedded-claude-architecture.md:995). `client_slug: Option<String>` matches Codex I3. `.envs(&env_vars)` additive (not replace) matches Section 2. Vault directory creation preserved (Codex C1). No unauthorized scope expansion beyond the spec's §Out-of-scope list.
7. Readiness:      PASS — Frontend wraps the re-auth flow with a 5-minute cancel timeout and listens for the existing `oauth:token-stored` event to chain kill+reconnect. Tests shipped: 4 `mcp_config` unit tests, 5 `stream_parser` auth-error tests, 1 `session_manager` kill test preserved. Build compiles at 3b HEAD (TypeScript strict-null errors in src/lib/mcp-utils.ts are from Phase 3c commit 44e3690 and are out of scope for this review).

DECISION: APPROVED
Score: 9/10
Conditions: None blocking. One non-blocking carry-forward noted in Risk register item above (caught and fixed in Phase 3c commit 26dbb71).
```

---

## Per-Task Verification

| Task | Spec intent | Evidence in shipped code | Verdict |
|------|-------------|--------------------------|---------|
| 1. Create `mcp_config.rs` | New module generates per-engagement `.mcp-config.json` with atomic tmp+rename; 4 unit tests | `src-tauri/src/claude/mcp_config.rs` present; `generate_mcp_config()` signature + tmp/rename logic intact; tests `test_generate_config_{with_google_token,no_token,no_vault,empty}` present at lines 110–177 | PASS (note: current main adds optional `npx_path` param from Phase 4a/b89e820 — backward-compatible) |
| 2. Extend `session_manager.spawn()` | Add `env_vars: HashMap` + `mcp_config_path: Option<String>` params, `.envs()` on Command, push `--mcp-config` args | `session_manager.rs:31-39` signature matches; `args.push("--mcp-config")` at lines 79-82; `.envs(&env_vars)` at line 110; `.env("PATH", full_path)` added later in Phase 4a is additive and does not regress 3b intent | PASS |
| 3. Orchestrate in `commands.rs` | Keychain read → vault dir create → config gen → spawn → registry | Phase-3b snapshot (d1442ef) matches plan exactly. Current main uses `oauth::token_refresh::refresh_if_needed` (Phase 4a/76a8b26) which is an enhancement preserving the same semantic (token in env_vars). `client_slug: Option<String>` present (I3) | PASS |
| 4. Retire McpProcessManager (Rust) | Delete `src-tauri/src/mcp/` + commands/mcp.rs; scrub `lib.rs` + `commands/mod.rs` | `ls src-tauri/src/mcp` → directory does not exist. `commands/mod.rs` has only `credentials`, `oauth`, `vault`. `lib.rs` has no `mod mcp;`, no `McpProcessManager`, no 5 MCP command handlers. | PASS |
| 5. Retire frontend MCP + `useEngagement.ts` | Delete hook, remove 5 wrappers, update `spawnClaudeSession` signature | `src/hooks/useEngagement.ts` does not exist. Grep with word-boundary for `\buseEngagement\b|\bspawnMcp\b|\bkillMcp\b|\bkillAllMcp\b|\bmcpHealth\b|\brestartMcp\b` across `src/` → zero matches. `spawnClaudeSession` in `tauri-commands.ts:75-89` adds `clientSlug` (also `strictMcp` from Phase 3c — backward-compatible optional). | PASS |
| 6. `useWorkspaceSession.ts` clientSlug plumbing | 4 call sites (2 in connect, 2 in switchEngagement) pass `client?.slug` | `useWorkspaceSession.ts:73-80, 93, 137-148, 158` — all 4 call sites resolve client from `engagementStore.clients` and pass slug. Clean. | PASS |
| 7. `McpAuthErrorPayload` types | Rust struct + TS interface with `server_name` + `error_hint` | `claude/types.rs:167-171` + `src/types/claude.ts:82-85`. Field names match (snake_case on wire, as expected for Serde default). | PASS |
| 8. Auth-error detection in stream_parser | Match 401/403/token expired/etc.; emit `claude:mcp-auth-error` | `stream_parser.rs:282-300` + helpers at 306-332. Keywords list matches spec. 5 unit tests present (lines 412-435). **Latent bug fixed in 3c**: spec plan said `infer_mcp_server(tool_id)` but tool_id is an opaque `toolu_…` string — re-keyed to `tool_name` via `tool_name_map` in Phase 3c commit 26dbb71 (Codex C2 catch). | PASS (with carry-forward) |
| 9. Frontend auth-error state + listener | `authError` state + setter/clearer; `claude:mcp-auth-error` listener | `claudeStore.ts:11, 25-26, 40, 149-153`. `useClaudeStream.ts:101-108` listens and dispatches. | PASS |
| 10. ChatView toast + re-auth flow | Amber banner; button triggers OAuth + kill + reconnect | `ChatView.tsx:37-104, 168-183`. Uses `startOAuthFlow`/`cancelOAuthFlow`, listens for `oauth:token-stored` (existing OAuth infra), kills session, calls `handleConnect()`. 5-min cancel timeout present. | PASS |
| Wave 4. Parent spec amendment | `embedded-claude-architecture.md` Q3 says "session spawn time" not "scaffold time" | Confirmed at line 995: "…per-engagement config file generated at session spawn time (not scaffold time — token availability changes between scaffold and spawn)." | PASS |

## Codex Findings Resolution Audit

| ID | Origin | Claimed resolution | Actually landed? |
|----|--------|--------------------|------------------|
| C1 | Design review | Vault dir creation moved to Rust orchestrator | Yes — `commands.rs` step 2 creates `~/.ikrs-workspace/vaults/{slug}` before config gen. 6124d78 upgraded `let _ = ` silent failure to `log::warn!` |
| I1 | Design review | Parent spec amended to "spawn time" | Yes — d1442ef |
| I2 | Design review | `spawn()` gains env vars + `--mcp-config` | Yes — Task 2 |
| I3 | Checkpoint 1 | `client_slug: Option<String>` | Yes — engagements without clients skip MCP generation entirely (verified in `commands.rs:36`) |
| I1 (ckp1) | Checkpoint 1 | mcpStore consumer audit: empty servers handled | Confirmed — `useDrive/useNotes/useGmail/useCalendar` all gate on `.find()` returning a truthy server; empty list = feature dormant, no crash. Phase 3c (44e3690 + 1b26737) later wires `setServers` via `system.init` parsing, upgrading dormant → functional |

## Documentation Enforcement (Golden Rule #12)

- [x] Plan status updated — `docs/superpowers/plans/2026-04-12-m2-phase3b-mcp-wiring.md` front matter: "Status: Complete — all 10 tasks implemented, 12 commits on main"
- [x] Architecture docs current — parent spec `embedded-claude-architecture.md` Q3 amended (d1442ef)
- [x] Spec amended — phase spec `m2-phase3b-mcp-wiring-design.md` marked "Status: Complete"
- [x] Risk register — all 6 in-spec risks tracked; new note added here for 3b→3c tool_id→tool_name fix
- [x] CLAUDE.md — no new rule emerged; no update required
- [x] `.gitignore` — `.mcp-config.json` + `.mcp-config.json.tmp` entries added (66dfb10)
- [ ] Session handoff — no 3b-specific handoff in `.output/` but 3c continuity commits pick up directly; not a blocker for phase-review verdict

## Residual Observations (non-blocking)

1. **Keychain read uses `.ok().flatten()`** — swallows keychain errors silently. If keyring is uninitialized on first launch the user gets "MCP config with no Google tools" rather than a diagnostic. Low severity; acceptable per spec's "expired tokens are included" graceful-degradation posture but worth a future `log::debug!` hook.
2. **`tool_name_map` lives in parser-local state** — correct for a single session but if `parse_stream()` is ever re-entered mid-session the map is reset. Not exercised by current architecture (one parser per spawn) but worth noting should multi-parser support ever land.
3. **macOS sandbox npx resolution** — Phase 3b shipped with bare `"npx"` command (correct at the time). Phase 4a/b89e820 retrofit added `npx_path` param to `generate_mcp_config`. 3b's review stands because 3b targeted dev mode; sandbox was explicitly deferred per spec Risks row 3.

## Final Decision

**APPROVED — 9/10**

All 10 planned tasks shipped. All Codex findings (C1, I1, I2, I3, checkpoint-1 I1) substantively resolved. Architecture is clean, security posture is correct, tests present, parent spec amendment landed. One carry-forward bug (tool_id → tool_name keying in auth-error inference) was self-caught in Phase 3c before any user impact — it costs one point off a perfect score but does not warrant FAIL or conditions because the fix is on `main` and never shipped to a release build.

No follow-up commit required.
