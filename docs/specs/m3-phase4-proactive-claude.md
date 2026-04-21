# M3 Phase 4 — Proactive, evolving, immersed Claude

**Status:** DRAFT — awaiting Codex plan-gate review
**Target:** 3 consecutive sub-phases, each independently shippable
**Deploy:** non-disruptive (build+push; Moe quits+reopens)
**Owner:** Claude Opus 4.7 (this session)
**Trigger:** Moe 2026-04-21 — *"i'm back, had to quit and reopen for some updates to reflect / Welcome back! Let me check where we left off. / Fresh session — no prior memory saved for this vault yet."*

## Problem

After a `Cmd+Q → reopen`, Claude's first message defaults to *"Fresh session — no prior memory saved for this vault yet. What would you like to work on?"* That's technically accurate — each Claude CLI spawn is a stateless process — but it's the opposite of what a consultant-facing assistant should feel like. Moe wants the agent to:

1. **Open proactively** with today's state (calendar, priority emails, open tasks, recent notes) — not ask "what do you want to do?".
2. **Evolve across sessions** — remember preferences, accumulated lessons, relationship context. Get better over time.
3. **Use Obsidian as its working surface fully** — structured templates, daily notes, meeting notes, decisions, `_memory/` — not freeform markdown dumps.

## Non-goals

- Multi-engagement memory sharing. Each engagement has its own `_memory/`. Cross-engagement patterns may emerge later but aren't v1.
- Client-portal exposure of `_memory/`. The memory folder is consultant-only and must be excluded from any client-visible sync.
- Replacing the Claude Code CLI with an SDK-embedded agent. Still spawning the CLI subprocess; adding context at spawn time only.
- Training / fine-tuning. Memory is a textual context injection, not weight updates.

## Current state (verified 2026-04-21 via code audit)

| Capability | State | Evidence |
|---|---|---|
| MCP: Gmail | ✅ wired | `src-tauri/src/claude/mcp_config.rs` — @shinzolabs/gmail-mcp with OAuth creds |
| MCP: Drive | ✅ wired | same file — @piotr-agier/google-drive-mcp |
| MCP: Obsidian | ✅ wired | same file — @bitbonsai/mcpvault@0.11.0 |
| MCP: Calendar | ❌ broken | same file — @cocal package needs a tokens.json file in a format we never built |
| Session-boot briefing | ❌ absent | `src-tauri/src/claude/commands.rs:spawn_claude_session` — spawns CLI with no contextual priming |
| Evolving memory | ❌ absent | No `_memory/` convention. No session-end distiller. CLAUDE.md is manual-edit only. |
| Vault structure | freeform | No templates, no scaffolder, no daily-note auto-creation |

## Just-shipped context (this session's prior work)

Ship-readiness of this plan depends on these prior commits landing cleanly — they do:

- `7c10d22` E2E hardening batch (vault bridge, shareEvents reader, ResizableLayout, mcp-config unlink, cost ledger)
- `466354d` Codex P1/P2 fixes (firestore rules anchored, shareEvents ownership gate, timesheet events gate, Windows rename rescue, per-task vault-mirror queue)
- `be00ef9` Windows rename data-loss fix
- Live Firestore: new rules released via `firebase deploy --only firestore:rules`

Relevant dependencies for this plan:
- `write_task_frontmatter` (Rust) — atomic file write with backup-restore fallback. Reusable for any vault write.
- `markTaskPendingLocal` + 2s anti-flicker window — suppresses watcher echo after vault writes. Must extend for briefing's daily-note creation.
- Per-task mirror queue (useTasks) — pattern for serializing vault writes. Extend to other vault mutations.
- OAuth refresh pipeline (`src-tauri/src/oauth/`) — already rotates Google access tokens. Reusable for Calendar REST in Phase A.

---

## Phase A — Proactive session-boot briefing

**Target:** ~1 day · **Ordering:** ship first · **User-visible win:** biggest

### Behavior

On every Claude session spawn (new or resume), the Rust backend aggregates the consultant's current-state snapshot and injects it as the first message Claude sees. Claude opens with something like:

> "Good morning. You've got 3 things today: (1) 14:00 ops review with BLR — agenda doc not yet created; (2) 2 emails flagged from Sarah Chen at BLR, both about the venue change; (3) task `procure-av-stack` is blocked on budget approval. Want me to draft the agenda and reply to Sarah first, or stay quiet while you get coffee?"

Instead of: *"Fresh session — what would you like to work on?"*

### Data sources

| Source | Call | Notes |
|---|---|---|
| Today's Google Calendar | REST v3 `events.list` with `timeMin=now`, `timeMax=endOfDay`, `singleEvents=true`, `orderBy=startTime` | Direct REST — bypasses the broken @cocal MCP. Scope already in `GOOGLE_SCOPES` array (`calendar.events`). |
| Priority unread emails | Gmail REST v1 `users.messages.list` with `q=is:unread (label:important OR label:starred) newer_than:1d` then `messages.get` for top ~5 with `format=metadata` | Scope `gmail.modify` already granted. |
| Active tasks | Firestore `ikrs_tasks` where `engagementId = X AND status in [in_progress, blocked, awaiting_client]` | Decision: use the already-running frontend listener and pass the snapshot to Rust via IPC on spawn. Avoids duplicating Firestore auth in Rust. |
| Recent vault notes | Filesystem — walk `vault_path`, collect top 3 `.md` files by mtime excluding `_memory/` and `.trash/` | Uses existing vault_path resolution. |

### Architecture

```
[spawn_claude_session] (Tauri command)
    │
    ├── collect OAuth creds (existing)
    ├── NEW: briefing::collect(engagement_id, vault_path, google_oauth, active_tasks_snapshot)
    │         returns BriefingPayload { calendar, emails, tasks, notes, warnings }
    ├── NEW: briefing::render(payload) -> String (markdown-formatted)
    ├── spawn claude subprocess
    └── NEW: session_manager::prime(session_id, briefing_text)
              writes to the CLI's stdin as a synthetic first user message,
              preceded by a hidden marker ("<<BRIEFING_CONTEXT v1>>") so
              the UI's existing stream parser can hide the echo if we
              choose to suppress it in the chat transcript.
```

### Frontend

- `useWorkspaceSession.ts` collects `tasks` snapshot (already subscribed) and passes to `spawn_claude_session`.
- `ChatView.tsx` — no UI change by default. The briefing arrives as an assistant turn (Claude's proactive open), which the existing MessageBubble already renders.
- Optional: `useBriefingToggle` store — per-engagement "show raw briefing context" for debugging. Default off.

### Non-disruptive deploy considerations

- Adds a synthetic first message that previous sessions did not have. If Moe reconnects to an existing CLI session (resume path), we should NOT re-inject the briefing — spawn path only.
- Briefing adds ~2–5k tokens per session. Cost ledger tracks this already; no additional plumbing.
- If any data source fails (Calendar API down, vault unreachable, Firestore listener not yet warm), the aggregator degrades gracefully — missing sections are omitted with a short `warnings` note at the bottom, not a hard failure.

### Phase A success criteria

1. After `Cmd+Q → reopen`, Claude's first message references today's calendar or open tasks by name (not a generic "fresh session" opener).
2. If Calendar API is offline, Claude still gets tasks + emails + notes and opens with what's available. No wedge.
3. Token spend per session-boot is ≤ $0.02 (measured via cost ledger) on a typical day.
4. Briefing surfaces only the active engagement's data — no cross-engagement leakage.

---

## Phase C — Full Obsidian wiring

**Target:** ~1 day · **Ordering:** ship second · **Why before B:** memory in Phase B needs a folder structure to live in. Phase C creates the structure.

### Behavior

When an engagement is created (or on first app launch after this phase ships, for existing engagements), the vault gets scaffolded with a standard structure. Daily notes are auto-created on session boot. Claude's system prompt is amended with conventions so it saves work into the right folders without being told each time.

### Vault structure

```
{vault_path}/
├── 00-index.md              ← engagement overview, quick links
├── daily-notes/             ← YYYY-MM-DD.md, auto-created on session boot
├── meetings/                ← YYYY-MM-DD-slug.md (attendees, agenda, decisions, actions)
├── decisions/               ← NNN-title.md (sequential; one per strategic/architectural call)
├── briefs/                  ← quick-prompt artifacts
├── 02-tasks/                ← existing — untouched; task_watch continues to rule this folder
└── _memory/                 ← created empty here; populated by Phase B
    ├── principles.md
    ├── lessons.md
    ├── relationships.md
    └── context.md
```

### Templates

Each folder gets a template file checked in to `src-tauri/src/vault_templates/`. Compiled into the binary (no runtime dep) — bytes copied into the vault at scaffold time. Templates are plain markdown with YAML frontmatter where applicable.

### Scaffolder behavior

- Triggered by `create_engagement` Tauri command (new engagement) AND by a one-time idempotent migration on session spawn for existing engagements (`vault_scaffold::ensure(vault_path)`).
- **Idempotent by design**: if `00-index.md` exists, leave it alone; if `daily-notes/` exists, leave its contents alone. Never overwrites.
- Uses the same atomic-write pattern as `write_task_frontmatter` (tmp + rename + backup on failure).
- Logs every file created to the Tauri log so Moe can see what the scaffolder did.

### Daily note auto-creation

- At session boot, after briefing collection, check `daily-notes/YYYY-MM-DD.md` (local TZ).
- If missing, create from template with today's date + title + empty sections for "What's on today", "Decisions", "Tomorrow".
- Uses `markTaskPendingLocal`-equivalent mechanism to suppress the watcher echo.

### Calendar MCP decision

Two options:

**Option C1** — Fix @cocal package: write `~/.config/google-calendar-mcp/tokens.json` in the expected format at session spawn. Requires reverse-engineering the @cocal TokenManager schema. Medium complexity.

**Option C2** — Replace with a direct-REST Tauri command: `get_calendar_events(engagement_id, timeMin, timeMax) -> Vec<Event>`. Exposed as a tool to Claude via a new minimal MCP shim, OR called internally from the briefing aggregator in Phase A.

**Recommendation: C2.** Phase A's aggregator already does Calendar REST. Reusing that path avoids a second code path for the same API and removes one moving part (the @cocal package). The narrow downside: Claude can't query calendar interactively via MCP during the session — only briefed at boot. Mitigation: expose a dedicated `calendar_query` Tauri command Claude can invoke via a tiny shim MCP server, or bundle a short "calendar summary" section at every turn if needed. Revisit if users hit the limit.

### Phase C success criteria

1. New engagement creation produces the full folder structure with templates.
2. Existing engagements get the folders on first session-boot after this ships — no data loss, no overwrites.
3. Daily note for today exists by the time Claude's briefing arrives.
4. Claude saves meeting notes to `meetings/` and decisions to `decisions/` without being told per session.
5. Calendar data surfaces in the briefing reliably (Phase A path confirmed still works).

---

## Phase B — Evolving memory

**Target:** ~1.5 day · **Ordering:** ship third · **Why last:** needs the folder (Phase C) and the briefing pipeline (Phase A) to plug into.

### Behavior

`_memory/` holds four markdown files Claude reads at session boot and writes to at session end. Each file has a specific role:

| File | Role | Example entry |
|---|---|---|
| `principles.md` | How Moe works — preferences, communication style, non-negotiables | "Moe wants briefings in under 200 words. Longer goes into a `briefs/` file with a link." |
| `lessons.md` | Gotchas learned the hard way. Append-only with date stamps. | "2026-04-18 — Don't suggest calendar-MCP fixes that require manual token-file pre-population; it wedges connecting." |
| `relationships.md` | Who's who on the client side. One section per person. | "## Sarah Chen — BLR Ops Lead\n- Prefers email over phone.\n- Decision owner for venue logistics." |
| `context.md` | Current engagement state — live, overwritten, not append-only. | "Current focus: BLR Phase-2 pre-production. Blocker: budget approval from finance." |

### Read path (session boot)

After Phase A's briefing collection, the backend reads all four files (if they exist) and appends them to the briefing payload. Rendered in a hidden section at the top of the first turn so Claude has the context but the UI can hide it.

### Write path (session end)

On session exit (any reason — clean, crash, user-initiated kill), the session_manager spawns a secondary, ephemeral Claude CLI call with a narrow prompt:

> "Review this session's transcript (passed as stdin). For each of the four categories — principles, lessons, relationships, context — identify new entries that the consultant or you discovered in this session. Emit strict YAML: `{principles: [...], lessons: [{date, entry}, ...], relationships: {name: "...", notes: "..."}, context_update: "..."}`. If a category has no new entries, return an empty list/object."

The emitted YAML is parsed by the Rust distiller and merged into the four files:
- `principles.md` — new bullets appended under a "Session {date}" sub-header.
- `lessons.md` — append-only list of `{date, entry}`.
- `relationships.md` — new people added; existing people's `notes` section appended to.
- `context.md` — **overwritten** with the new context (the distiller is asked for the full current state, not a diff — so stale context naturally gets replaced).

### Size control

- Each file capped at ~200 lines post-merge. When exceeded, oldest entries (by date stamp) rotated to `_memory/archive/YYYY-MM.md`.
- Distiller prompt explicitly asks: "if you're about to add something that's already there, skip it" — dedup at emit time.
- Soft budget: session-end distiller is ≤ 1 Claude turn. Cost ≤ $0.05 per session worst-case.

### Safety

- `_memory/` writes use the same atomic tmp+rename+backup pattern.
- If the distiller's output fails to parse as YAML, we skip the update silently and log the failure — never corrupt the memory files.
- `_memory/` path excluded from any client-portal sync (future work — not yet built, but design accounts for it).
- Distiller prompt explicitly forbids storing anything a client shouldn't see (the client portal gate is later; this is belt-and-suspenders).

### Phase B success criteria

1. After 5 sessions with the distiller running, `_memory/` has non-trivial content that reflects actual patterns from those sessions.
2. Distiller failure (bad YAML, timeout) does not block the primary session from completing cleanly.
3. Next session's Claude references a `_memory/` entry from a prior session within the first two turns.
4. Total token cost per session (briefing + memory + distiller) ≤ $0.10 on a typical-length day.

---

## Cross-phase concerns

### Cost

- Phase A briefing: ~2–5k tokens/session → ~$0.01–0.03.
- Phase B memory injection: ~1–3k tokens/session → ~$0.005–0.015.
- Phase B distiller call: ~2–4k tokens + 1 Claude turn → ~$0.03–0.05.
- **Worst case per session:** ~$0.10. Heavy day (20 sessions) → ~$2. Within Max quota.

### OAuth scopes already granted

From `ChatView.tsx`:
- `gmail.modify` ✅ covers message list + get
- `calendar.events` ✅ covers events.list
- `drive.readonly` ✅ covers Drive MCP

No new OAuth consent required.

### Race conditions

- Briefing's vault-note scan runs before Claude spawns → no race with Claude's reads.
- Daily-note auto-create in Phase C must precede briefing render (so the note is in the "recent notes" list). Sequencing enforced in Rust.
- Session-end distiller (Phase B) spawns its own Claude subprocess — independent of the main session. Must NOT share the MCP config (that file was just unlinked on session exit per `466354d`). Distiller runs without MCP — pure transcript-in, YAML-out.

### Watcher interactions

The existing `notify`-based task watcher only watches `02-tasks/`. None of the new folders (`daily-notes/`, `meetings/`, etc.) are watched, so the scaffolder's writes don't fire spurious events. `_memory/` is intentionally unwatched — Claude's writes to it are internal.

### Windows

All new file writes reuse the backup-restore rename pattern from `task_watch.rs:328` so Windows users don't hit data loss.

---

## Proposed ordering

**A → C → B.**

- **A first** because it's the biggest user-visible win and has no dependencies. Ships day 1.
- **C second** because it creates the folder structure B needs. Ships day 2.
- **B last** because it's the most complex (distiller, YAML parsing, file merging) and every failure mode must be non-blocking for the primary session.

---

## Open questions for Codex plan-gate review

1. Is the IPC pattern (frontend passes `active_tasks_snapshot` to Rust spawn command) the right call, or should Rust query Firestore directly?
2. Should briefing be injected as first-user-message (Claude sees it, responds to it) or as `--append-system-prompt` (Claude treats it as silent context)?
3. Session-end distiller: are we sure Claude's CLI surfaces a clean hook for "spawn a separate one-shot call with transcript as input"? Risk is that we'd need to resume the session and kill it again, which is wasteful.
4. Is `_memory/` the right name, or would `0-context/` / `.claude-memory/` serve better? Leading underscore should hide it from typical Obsidian folder views; confirm.
5. For existing engagements that pre-date this phase, is the "scaffolder runs at session spawn if folders missing" pattern safe — or should it require an explicit `migrate_engagement` command the user opts into?

---

## Rollback plan

Each phase goes out as one commit, reviewed by Codex pre-push. If any phase causes a regression in the live app:

- **Phase A rollback** — feature-flag the briefing collector (`ENABLE_BRIEFING` env var, default on). Flip to off; spawn proceeds without briefing injection, app behaves as it did pre-Phase A.
- **Phase C rollback** — scaffolder is additive only. A bad template just produces a worthless file which the user can delete. No rollback needed for correctness; a patch commit fixes the template.
- **Phase B rollback** — distiller failure is already silent. The memory-injection path can be feature-flagged too (`ENABLE_MEMORY_INJECTION`). If `_memory/` content turns out to steer Claude wrong, user can delete the file and distiller starts fresh.
