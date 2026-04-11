# Codex Architectural Assessment -- M2 Phase 3a/3b Design Decomposition

**Reviewer:** Codex (Claude Opus 4.6)
**Date:** 2026-04-11
**Verdict:** PASS 8/10

## Conditions (W1-W5) from Prior Review: ALL RESOLVED

| Condition | Resolution |
|-----------|------------|
| W1: C1 (session map leak) as task 1 | Phase 3a item 1 |
| W2: C2 (unified hook) confirmed | Phase 3a item 6: `useWorkspaceSession()` |
| W3: Phase 3a/3b split | Confirmed |
| W4: JSON vs SQLite | JSON file at `{app_data_dir}/session-registry.json` |
| W5: Non-strict `--mcp-config` | Additive mode (consultant's personal MCPs preserved) |

## Phase 3a Task Ordering: CORRECT

Dependency chain validated:
- Item 1 (C1 fix) → prerequisite for all session management
- Items 2-3 (UI) → independent, parallelizable
- Item 4 (chat history) → prerequisite for item 6
- Item 5 (session resume) → prerequisite for item 6
- Item 6 (orchestrator) → depends on 1, 4, 5
- Item 7 (orphan cleanup) → after item 5 (shared registry)

## Architecture: `useWorkspaceSession()` as Hook — CORRECT

Hook is the right abstraction (not store action) because it needs Tauri IPC calls.

## Advisory Items (incorporate during spec writing)

**A1:** Clarify `useEngagement` retirement path — `EngagementSwitcher` imports `useWorkspaceSession`.
**A2:** Put Obsidian in `.mcp-config.json` alongside Google MCPs. Mark `McpProcessManager` for full removal in Phase 3b.
**A3:** Add `tool_result_full: Option<String>` (2KB cap) to `ToolEndPayload` for collapsible card.
**A4:** Use atomic write (temp file + rename) for JSON registry. Handle parse errors as empty registry.

## Key Decisions

- Obsidian MCP: CLI-managed via `.mcp-config.json` (Option A)
- `McpProcessManager`: Full removal in Phase 3b
- `tool_input`: Store as `Option<String>` (JSON-serialized, 4KB cap) not `serde_json::Value`
- `tool_result_full`: `Option<String>` (2KB cap) on `ToolEndPayload`
- Orchestrator kill: Use `kill_claude_session` IPC (synchronous HashMap removal) not stdin EOF
