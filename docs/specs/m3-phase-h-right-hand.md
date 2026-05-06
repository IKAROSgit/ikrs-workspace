# M3 Phase H — Autonomous Right-Hand Daily Study Session

**Status:** DRAFT — pre-code, adversarial challenge pending (§9)
**Depends on:** Phase F (shipped), Phase G (spec locked, G.2 implemented)
**Branch:** `phase-h-right-hand`

## 1. Goal + scope

Build an autonomous daily study session that runs as a Claude Code
subprocess on the operator's Mac. It reads all available surfaces
(vault, calendar, email, Google Chat, heartbeat audit log), distills
knowledge into persistent memory, drafts communications, and produces
a daily brief — all without human interaction.

**What it is:** A scheduled daily "right-hand" session that learns
about the operator's work, builds a knowledge graph, and surfaces
actionable intelligence.

**What it is NOT:** A real-time reactive agent (that's Phase G's bot
poller + proactive Claude). Phase H is a deep, slow, thorough study
that runs once daily during quiet hours.

## 2. Architecture — 3 personas

| Persona | Where | When | Role |
|---|---|---|---|
| **Heartbeat (Tier II)** | elara-vm, Python | Hourly 24/7 | Triage + Kanban tasks. Reads Gmail/Calendar/vault. Lightweight. |
| **WS Claude interactive** | Mac Tauri app, Claude subprocess | Human present | Chat-based coding/research sessions. Human-driven. |
| **WS Claude autonomous** (Phase H) | Mac Tauri app, Claude subprocess | Scheduled daily, unattended | Deep study. Reads all surfaces. Writes to memory + drafts. Produces daily brief. |

The autonomous session uses the same Claude Code subprocess
infrastructure as interactive sessions (`spawn_claude_session`) but
with a synthesised prompt (no human typing). It's registered via the
`scheduled-tasks` MCP server and fires once daily at a configured time
(default: 05:00 local, before the operator wakes).

**Key constraint:** Runs under Anthropic's consumer terms because the
Mac is the operator's personal device and the operator has opted in to
the scheduled task. The session is not "unattended automation on
infrastructure" — it's "a scheduled task on the user's own computer,
like a cron job." Same posture as macOS launchd agents.

## 3. Memory directory structure

```
operations/
├── memory/
│   ├── README.md                     # How to read/write/audit this directory
│   ├── identity/
│   │   └── moe.md                    # Operator identity + preferences
│   ├── preferences/
│   │   └── style.md                  # Communication style + format rules
│   ├── engagements/
│   │   └── blr-world.md             # Per-engagement context
│   ├── people/
│   │   └── README.md                # Key contacts the model encounters
│   ├── projects/
│   │   └── README.md                # Active project summaries
│   ├── loops/
│   │   └── README.md                # Open loops (things waiting on replies)
│   ├── patterns/
│   │   └── README.md                # Recurring observations over time
│   ├── decisions/
│   │   └── README.md                # Key decisions made + rationale
│   └── recent/
│       └── README.md                # Last 7 days rolling context
├── right-hand/
│   ├── daily-prompt.md              # The autonomous session prompt template
│   ├── checkpoint.json              # Resume state if cap-hit mid-session
│   └── briefs/
│       └── YYYY-MM-DD.md            # Daily brief output
├── drafts/                           # All auto-generated drafts land here
│   └── README.md
└── audit/
    └── daily-session-YYYY-MM-DD.jsonl  # Per-session audit log
```

All paths relative to `~/.ikrs-workspace/vaults/blr-world-com/` (the
active engagement's vault root).

## 4. Surfaces studied

| Surface | API/Tool | Scope | Auth |
|---|---|---|---|
| Vault filesystem | Direct read (Claude's cwd) | Full vault tree | Filesystem (Claude's sandbox) |
| Google Calendar | `calendar.events.list` | Next 7 days + last 24h | Per-engagement OAuth (Phase F) |
| Gmail | `gmail.users.threads.list` | Last 24h unread/starred | Per-engagement OAuth |
| Google Chat | `chat.spaces.list` + `chat.spaces.messages.list` | Last 24h across all spaces | Per-engagement OAuth (new scopes in H.2) |
| Heartbeat audit log | Direct read `_memory/heartbeat-log.jsonl` | Last 7 days | Filesystem |
| Kanban (Firestore) | MCP or direct Firestore read | All open tasks for engagement | Claude session context |

## 5. Daily prompt phases

### Phase 0 — Boot
- Check kill-switch: if `operations/right-hand/KILL` exists, exit immediately
- Read checkpoint: if `operations/right-hand/checkpoint.json` exists and
  `date == today`, resume from `last_completed_phase`
- Load identity: `operations/memory/identity/moe.md`
- Load engagement context: `operations/memory/engagements/blr-world.md`
- Load preferences: `operations/memory/preferences/style.md`

### Phase 1 — Memory refresh
- Read all files in `operations/memory/` (people, projects, loops,
  patterns, decisions, recent)
- Build working context of "what I know about Moe's world"

### Phase 2 — Surface walk
- **Calendar:** `calendar.events.list` for next 7 days + last 24h
  changes. Note new meetings, cancellations, time conflicts.
- **Gmail:** `gmail.users.threads.list` with `q=is:unread OR
  is:starred after:<24h_ago>`. Read top 25 thread snippets.
- **Google Chat:** `chat.spaces.list` → for each space,
  `chat.spaces.messages.list` with `filter="createTime >
  <24h_ago>"`. Note conversations requiring Moe's input.
- **Vault:** Walk recent changes (git diff or mtime-based).
- **Heartbeat audit:** Read last 7 days of
  `_memory/heartbeat-log.jsonl`. Note patterns, recurring items.

### Phase 3 — Memory updates
- Update `people/*.md` with new contacts encountered
- Update `projects/*.md` with status changes observed
- Update `loops/*.md` with new open items
- Update `patterns/*.md` with recurring observations
- Update `recent/` with today's rolling summary
- All writes atomic: write to temp file, rename into place
- All writes append-only (add new info, never delete existing)

### Phase 4 — Daily brief
Write `operations/right-hand/briefs/YYYY-MM-DD.md`:
- **Summary:** 3-5 bullet points of what happened in last 24h
- **Stale items:** Things that haven't moved in >72h
- **Open loops:** Awaiting replies from others
- **Calendar:** Next 24h meetings with prep notes
- **Anchor recommendation:** "If you only do one thing today, do X"
- **Decisions awaiting Moe:** Items that need human judgment

### Phase 5 — Draft generation
For any items surfaced in Phase 4 that have obvious next-actions:
- Draft replies → `operations/drafts/YYYY-MM-DD-<slug>.md`
- Never auto-send. Never write outside `operations/drafts/`.
- Each draft includes: TO, SUBJECT, CONTEXT, DRAFT BODY

### Phase 6 — Audit
Append to `operations/audit/daily-session-YYYY-MM-DD.jsonl`:
```json
{"ts":"...","action":"memory_write","target":"people/angelique.md","bytes":342,"tokens_est":85}
{"ts":"...","action":"brief_write","target":"briefs/2026-05-04.md","bytes":1200,"tokens_est":300}
{"ts":"...","action":"draft_write","target":"drafts/2026-05-04-reply-enbd.md","bytes":450,"tokens_est":112}
{"ts":"...","action":"session_complete","duration_s":240,"total_tokens_est":15000,"phases_completed":7}
```

### Phase 7 — Checkpoint clear
- Delete `operations/right-hand/checkpoint.json` (session complete)
- Exit cleanly

## 6. Cap-hit handoff + resume protocol

Claude Code Max has a daily token budget. If the session approaches
the cap mid-run:

**Detection heuristics:**
- Track estimated tokens consumed per phase
- If cumulative estimate > 80% of daily budget, trigger cap-hit
- If a tool call returns HTTP 429 or rate-limit error, trigger cap-hit

**Cap-hit procedure:**
1. Write checkpoint:
```json
{
  "date": "2026-05-04",
  "last_completed_phase": 3,
  "partial_state": {
    "surfaces_read": ["calendar", "gmail"],
    "surfaces_remaining": ["chat", "vault_diff"],
    "memory_updates_pending": ["loops/enbd-clarification.md"]
  },
  "tokens_used_estimate": 12000,
  "resume_after": "2026-05-04T08:00:00+04:00"
}
```
2. Register resume task via `scheduled-tasks` MCP:
   `schedule_task(name="right-hand-resume", time=resume_after, prompt=<resume_prompt>)`
3. Exit cleanly with audit line: `{"action":"cap_hit","phase":3,...}`

**Resume:** Phase 0 boot detects the checkpoint, skips completed
phases, continues from `last_completed_phase + 1`.

## 7. Schedule registration

The daily session is registered via the `scheduled-tasks` MCP server
(available in Claude Code). The operator (or an initial setup session)
calls:

```
schedule_task(
  name: "right-hand-daily",
  schedule: "0 5 * * *",  // 05:00 local daily
  prompt: <contents of operations/right-hand/daily-prompt.md>
)
```

The MCP server manages scheduling, wake, and subprocess spawn.

## 8. Cost notes

| Item | Estimate |
|---|---|
| Daily session (full 7 phases, no cap hit) | ~15,000-25,000 tokens |
| Monthly (30 sessions) | ~500,000-750,000 tokens |
| Claude Code Max plan | 5x usage included |
| Cap-hit risk | Low at Flash-tier; moderate at Opus-tier |

Default model for the daily session: `claude-sonnet-4-6` (balance of
quality and cost). Operator can override in the prompt template.

## 9. Adversarial challenge findings

_Placeholder — populated after H.6 adversarial challenge runs._

## 10. Open questions / future hardening

1. **Quota budget cap:** Should the daily session have a hard token
   ceiling (e.g., 30,000 tokens/session) independent of the plan-level
   cap? Prevents runaway sessions from consuming the whole day's budget.
2. **Multi-engagement:** Current design is single-engagement (BLR). For
   multi-engagement, the daily session would iterate like the heartbeat.
   Deferred until a second engagement is active.
3. **Google Chat API availability:** Chat API requires a Google Workspace
   account with Chat enabled. Verify Moe's @blr-world.com account has
   this. If not, degrade gracefully (skip Chat surface, log warning).
4. **Notification on brief ready:** After Phase 4, push a Telegram
   notification: "Daily brief ready at operations/right-hand/briefs/
   YYYY-MM-DD.md". Requires Phase G bot poller integration.
5. **Memory size management:** As memory grows over months, Phase 1
   (memory refresh) will consume increasing tokens. Consider a monthly
   compaction pass that archives old entries.
