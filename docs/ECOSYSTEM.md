# IKAROS Workspace — Ecosystem & Architecture Reference

**This is the canonical doc.** Every AI agent and human contributor MUST
read this before making changes, and MUST update it in the same commit
when changes touch architecture, secrets, identity, schema, runbooks,
or phase status. See **`CLAUDE.md`** + **`AGENTS.md`** at repo root for
the enforcement rule.

> "If you didn't update ECOSYSTEM.md, you didn't finish the work."

Last verified: 2026-04-28 (Phase F.2 token sync, all sections reviewed).
See `git log -1 -- docs/ECOSYSTEM.md`. If the most recent commit to a
file in this list pre-dates an architecture-touching commit elsewhere,
this doc is stale and trusting it is unsafe.

---

## Integration coverage checklist

Every external integration / service / dependency the system uses
MUST have a dedicated, comprehensive section in this doc. "Mentioned
in passing across 3 sections" does not count — each integration
deserves its own block answering *why it exists, how it's set up,
how it's used, what it does NOT do, what's planned next*.

When you ADD a new integration, you MUST:
1. Add a row to the table below.
2. Write its dedicated section in the appropriate place in the doc.
3. Make sure both happen in the same commit.

When you REMOVE an integration, move its row to the "Removed" sub-
table (preserve history) and remove the section.

| Integration | Purpose | Doc section | Status |
|---|---|---|---|
| Tauri 2 + React frontend | Primary operator UI on Mac | §1, §3.1 | ✅ |
| Rust core (tokio, tauri-plugin-*) | Tauri runtime, OS integration | §1, §3.1 | ✅ |
| Tier I heartbeat (Rust tokio) | In-app cadence driver, while user present | §5.2 | ✅ |
| Tier II heartbeat (Python on VM) | 24/7 Gemini-driven triage | §5.1 | ✅ |
| Firebase Auth | Operator + client portal identity | §2 | ✅ |
| Firestore (client SDK) | Tauri reads/writes engagement-scoped data | §3.4 | ✅ |
| Firebase Admin SDK (service account) | Tier II writes telemetry + tasks | §3.4, §5.1 | ✅ |
| Firestore rules + indexes | Per-collection auth + query support | §3.4 | ✅ |
| Gmail API (read-only) | Tier II email collector | §5.1 | ✅ |
| Google Calendar API (read-only) | Tier II calendar collector | §5.1 | ✅ |
| Google OAuth (installed-app flow) | Per-engagement Gmail/Calendar auth | §3.1, §5.1 | ✅ (single-token in v1; Phase F multi-token) |
| Gemini 2.5 Pro (`google-genai` SDK) | Tier II LLM | §5.1 | ✅ |
| Telegram Bot API | Mobile push for urgent items | §5.3 | ✅ |
| BotFather + per-operator bots | Telegram bot provisioning | §5.3 | ✅ |
| Claude Code subprocess | In-app coding/chat sessions per engagement | §1, §3.1 | ⚠️ section thin — needs dedicated coverage |
| MCP servers (Gmail/Calendar/Drive/Obsidian) | Per-engagement tool access for Claude sessions | none | ❌ no dedicated section yet |
| Tailscale | VM access (mesh networking, identity-based SSH) | §3.2 | ⚠️ mentioned, no dedicated section |
| systemd timer + service | Tier II hourly cadence on VM | §3.2, §5.1 | ✅ |
| macOS keychain (tauri-plugin-keyring) | Per-engagement OAuth token storage on Mac | §3.1 | ⚠️ mentioned, no dedicated section |
| Hardened Runtime + entitlements | Mac code-signing posture | none | ❌ no dedicated section yet |
| GitHub Actions CI | Lint/test/build/docs-check | §3.3, §8 | ✅ |
| Phase E heartbeat audit log (JSONL) | Local per-tick + per-action audit | §3.2, §5.1 | ✅ |
| Phase F encrypted token sync | Per-engagement OAuth via Firestore (AES-256-GCM, WebCrypto TS + `cryptography` Python) | §3.4, §4, §5.4 | 🚧 F.1-F.6 landed; F.7 (adversarial review) + F.8 (deploy) pending |

**Status legend**: ✅ documented thoroughly • ⚠️ partial (sections
mention it but no dedicated block) • ❌ undocumented • 🚧 planned

The ⚠️ and ❌ rows are **technical debt in this doc itself** — they
should be fixed before adding more integrations on top.

### Removed integrations

(Empty for now. Move rows here with a note about when/why if any
integration is retired.)

---

## 0. What is IKAROS Workspace?

IKAROS Workspace is a consultant's professional desktop tool: a Tauri 2
app (Rust + React + TypeScript) running on the consultant's Mac that
gives them per-engagement vaults, a Kanban + notes UI, embedded Claude
Code subprocess sessions, MCP servers for Gmail/Calendar/Drive/Obsidian,
and an autonomous heartbeat agent that runs 24/7 (split between in-app
Claude when human-present and an on-VM Gemini service when the human is
away).

Designed for a single operator to start, but **commercially template-
ready** — each consultant gets their own VM, their own Gemini paid-tier
subscription, and the same code clone-and-go.

## 1. Big-picture architecture

```
┌──────────────────────────────────────────────────────────────────┐
│ Mac (operator's laptop)                                          │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │  IKAROS Workspace.app (Tauri 2)                            │  │
│  │                                                            │  │
│  │  ┌──────────┐  ┌──────────┐  ┌─────────────────────────┐  │  │
│  │  │ React UI │  │ Rust core│  │ Tier I heartbeat        │  │  │
│  │  │ (vite)   │←→│ (tokio)  │→→│ (tokio interval, hourly │  │  │
│  │  │          │  │          │  │  while app open)        │  │  │
│  │  └─────┬────┘  └─────┬────┘  └────────────┬────────────┘  │  │
│  │        │             │                    │               │  │
│  │        ↓             ↓                    ↓ emits         │  │
│  │  ┌────────────────────────────────────────────────────┐   │  │
│  │  │ Claude Code subprocess (per-engagement)            │   │  │
│  │  └────────────────────────────────────────────────────┘   │  │
│  └────────────┬───────────────┬──────────────────────────────┘  │
│               │               │                                 │
│        OAuth tokens     vault filesystem                        │
│        (keychain)       (~/.ikrs-workspace/vaults/)             │
└─────────────┬─────────────────┬─────────────────────────────────┘
              │                 │ scp + ssh                        
              │                 ↓                                 
              │     ┌──────────────────────────────────────┐      
              │     │ elara-vm (Debian, on Tailscale)      │      
              │     │                                      │      
              │     │  ┌────────────────────────────────┐  │      
              │     │  │ Tier II heartbeat (Python)     │  │      
              │     │  │ systemd timer, hourly 24/7     │  │      
              │     │  │ Gmail + Calendar + Vault →     │  │      
              │     │  │ Gemini → Firestore + Telegram  │  │      
              │     │  └────────────────────────────────┘  │      
              │     └──────────────────────────────────────┘      
              ↓                                                   
   ┌─────────────────────────────────────┐                        
   │ Firebase / Firestore (ikaros-portal)│                        
   │                                     │                        
   │  consultants, clients, engagements, │                        
   │  ikrs_tasks, taskNotes,             │                        
   │  heartbeat_health (Tier II writes,  │                        
   │  Tier I reads + verifies)           │                        
   └─────────────────────────────────────┘                        
```

Three layers:
- **Tauri app** — operator's UI, OAuth, Claude session host, vault writer.
- **Tier I (in-app heartbeat)** — runs while app is open. Verifies Tier
  II's writes via Firestore listeners and surfaces a status pill in
  Settings. Doesn't write to Firestore itself.
- **Tier II (on-VM heartbeat)** — runs hourly via systemd, 24/7. Reads
  email/calendar/vault, calls Gemini for triage, writes typed actions
  back to Firestore + sends Telegram pushes.

This split exists because Anthropic's Consumer Terms forbid unattended
Claude use, but Gemini's paid tier explicitly permits commercial /
non-human use. So we use Claude when the human is present and Gemini
when they're not.

## 2. Identity model

Three identity contexts, all distinct:

| Identity | Where | Purpose |
|---|---|---|
| **Consultant** (Firebase Auth UID) | Tauri app login, Firestore docs | The operator's primary identity. Owns engagements. |
| **Client portal user** (Firebase Auth UID) | Optional, separate login | If a client gives the consultant a user account inside their org's Google Workspace, that becomes a separate Firebase Auth UID. NOT used for Tauri-app login normally. |
| **Service account** (Firebase Admin SDK JSON) | VM only, `/etc/ikrs-heartbeat/firebase-sa.json` | The Tier II heartbeat writes via this. Bypasses client SDK rules. |

### Current production identities

- **Consultant**: `moe@ikaros.ae` → UID `yenifG1QiwVZtgNo42zaoSCPRTx1`
- **Client portal**: `moe@blr-world.com` → UID `BBxieDr3PqNn6hXQNbVuMirBdDF2`
- **Service account**: scoped to `ikaros-portal` Firebase project; key
  at `/etc/ikrs-heartbeat/firebase-sa.json` on elara-vm.

### Engagements (Firestore `engagements/{id}`)

Each engagement has `consultantId` pointing at a consultant's Firebase
Auth UID. The Tauri app filters `engagements` by
`where consultantId == request.auth.uid`. **Mismatched consultantId
makes the engagement invisible to the operator.**

- BLR World retainer: `5L12siRpQDDXnPCk892H` (consultantId =
  `yenifG1Q...` after Phase E remediation)

## 3. Where everything lives

### 3.1 Mac filesystem

| Path | Purpose | Owner |
|---|---|---|
| `/Applications/IKAROS Workspace.app` | Installed Tauri app | Operator install |
| `~/projects/apps/ikrs-workspace/` | Source repo (developer machine path) | Operator |
| `~/projects/apps/ikrs-workspace/.env.local` | Firebase config + `VITE_TOKEN_ENCRYPTION_KEY` (Phase F AES-256-GCM key, base64-encoded 32 bytes) (gitignored) | Operator, never committed |
| `~/.ikrs-workspace/vaults/<engagement-slug>/` | Per-engagement vault (markdown + assets) | Operator |
| `~/.local/bin/claude` (or other resolved path) | Claude Code CLI, used by Tauri's subprocess feature | Anthropic, installed via `npm i -g @anthropic-ai/claude-code` or similar |
| Mac keychain (entries `IKAROS Workspace://oauth/{engagementId}/google`) | Per-engagement Google OAuth tokens | Tauri app via tauri-plugin-keyring |
| `~/projects/apps/ikrs-workspace/heartbeat/.venv/` | Python 3.12+ venv (for one-time OAuth bootstrap on Mac) | Operator |
| `~/projects/apps/ikrs-workspace/heartbeat/token.json` | OAuth token freshly minted by `oauth_bootstrap`, before scp to VM. **Delete after scp.** | Transient |
| `~/.claude/` | Claude Code's per-machine settings + session storage | Anthropic |

### 3.2 VM filesystem (elara-vm, Debian)

| Path | Purpose | Mode | Owner |
|---|---|---|---|
| `~/projects/apps/ikrs-workspace/` | Code mirror (cloned from GitHub, on `main`) | 0755 | `moe_ikaros_ae` |
| `/etc/ikrs-heartbeat/heartbeat.toml` | Non-secret config (tenant_id, `[[engagements]]` array or legacy flat engagement_id + vault_root, LLM knobs) | 0640 | `ikrs:ikrs` |
| `/etc/ikrs-heartbeat/secrets.env` | Secret env vars: `GEMINI_API_KEY`, `TELEGRAM_BOT_TOKEN`, `TELEGRAM_CHAT_ID`, `FIREBASE_SA_KEY_PATH`, `TOKEN_ENCRYPTION_KEY` + `TOKEN_ENCRYPTION_KEY_VERSION` (Phase F) | 0600 | `ikrs:ikrs` |
| `/etc/ikrs-heartbeat/firebase-sa.json` | Firebase Admin SDK service account JSON | 0600 | `ikrs:ikrs` |
| `/etc/ikrs-heartbeat/google-token.json` | OAuth token (Phase E v1: single account; Phase F: replaced by Firestore-synced tokens) | 0600 | `ikrs:ikrs` |
| `/var/lib/ikrs-heartbeat/` | Reserved for state (currently unused; state lives in vault) | 0750 | `ikrs:ikrs` |
| `/opt/ikrs-heartbeat/venv/` | Python venv with heartbeat package + deps | 0755 | `ikrs:ikrs` |
| `/etc/systemd/system/ikrs-heartbeat.service` | systemd unit (Type=oneshot) | 0644 | `root:root` |
| `/etc/systemd/system/ikrs-heartbeat.timer` | systemd timer (OnUnitActiveSec=1h, Persistent) | 0644 | `root:root` |
| `/home/moe_ikaros_ae/vaults/<engagement>/_memory/heartbeat-state.json` | TickState (last_tick_ts, last_action_summaries, last_vault_mtimes, ...) | 0600 | `ikrs:ikrs` |
| `/home/moe_ikaros_ae/vaults/<engagement>/_memory/heartbeat-log.jsonl` | Append-only audit log: 1 line per tick + 1 line per action | 0644 | `ikrs:ikrs` |

VM accessed via Tailscale (`100.89.160.3` mapped to alias `elara-vm`).
SSH user: `moe_ikaros_ae`. Tailscale identity: `moe@ikaros.ae`. Sudo
works without password.

### 3.3 GitHub (IKAROSgit/ikrs-workspace)

- Main branch is the source of truth — both Mac and VM clone here.
- Phase work happens on `phase-X-...` branches, merged to main.
- GitHub email privacy is enabled on the operator's account; commits
  are authored as `IKAROSgit@users.noreply.github.com` to avoid
  rejection.
- CI: `.github/workflows/ci.yml` (lint, types, tests for JS/Rust/Python
  + a `test-heartbeat` job for the Python lane).
- Releases: tagged `vX.Y.Z`, build via `tauri-action`. Mac app is
  ad-hoc-signed (no notarization yet).

### 3.4 Firebase (project `ikaros-portal`)

| Collection | Purpose | Reader | Writer |
|---|---|---|---|
| `consultants/{uid}` | Consultant profile (name, role, prefs) | self | self |
| `clients/{id}` | Client orgs | authed | authed |
| `engagements/{id}` | Engagement record (consultantId, clientId, vault refs) | engagement consultant + client portal | engagement consultant |
| `ikrs_tasks/{taskId}` | Kanban tasks (manual + heartbeat-emitted). NOT to be confused with `tasks/` (Mission Control) | engagement consultant + clients with engagement access | engagement consultant + Tier II Admin SDK |
| `taskNotes/{noteId}` | Per-task notes | engagement consultant | engagement consultant |
| `taskNotes/{noteId}/shareEvents/{eventId}` | Append-only audit of share state changes | engagement consultant + clients | engagement consultant + clients |
| `engagements/{id}/google_tokens/{provider}` | AES-256-GCM encrypted OAuth tokens (Phase F). Plaintext = `{access_token, refresh_token, expires_at, client_id, client_secret}`. Schema: `{ciphertext, iv, keyVersion, updatedAt, writtenBy}`. See `docs/specs/m3-phase-f-token-sync.md`. | engagement consultant (client SDK) + Admin SDK | engagement consultant (client SDK on OAuth success) + Admin SDK (heartbeat refresh-token writeback) |
| `heartbeat_health/{tickId}` | Tier II telemetry, 30-day TTL via `expiresAt`. Tick ID format: `<tenantId>__<engagementId>__<tickTs-with-colons-replaced>` | any authed user | Admin SDK only (not client SDK) |
| `agent_sessions/...` | Claude session metadata | engagement consultant | engagement consultant |
| `subscriptions/{id}` | Billing | self | system |
| `onboarding/{id}` | Onboarding flow state | self | self |
| `timesheetSubmissions/{id}` + `events/{id}` subcollection | Timesheet submission + audit | consultant + client | consultant + client |
| `tasks/{id}` | **Mission Control** namespace, NOT used by ikrs-workspace. Reserved for IKAROS staff via `isIkarosStaff()` rule. | IKAROS staff | IKAROS staff |
| `agents/{id}` | Mission Control agents | IKAROS staff | IKAROS staff |

Rules: `firestore.rules` at repo root. Indexes:
`firestore.indexes.json` (currently the heartbeat_health composite
index). Both deployed via `npx firebase-tools deploy --only
firestore:rules` and `firestore:indexes`.

## 4. Phase status

| Phase | Title | Status | Notes |
|---|---|---|---|
| M1 | Auth + onboarding + engagements | shipped | |
| M2 | Vaults + Claude subprocess + MCP servers | shipped | |
| M3 Phase 1-3 | Kanban v1, notes, files, calendar | shipped | |
| M3 Phase 4 (timesheets) | Pending design | pending | |
| **M3 Phase E** | **Autonomous heartbeat (dual-tier)** | **shipped, soaking on elara-vm** | Spec: `docs/specs/m3-phase-e-autonomous-heartbeat.md` |
| **M3 Phase F** | **Multi-engagement OAuth via Firestore-synced tokens** | **in progress — F.1 spec locked, F.2-F.8 to follow** | Spec: `docs/specs/m3-phase-f-token-sync.md`. Pre-code adversarial challenge passed (3 showstoppers fixed). Tauri writes AES-256-GCM encrypted tokens to `engagements/{eid}/google_tokens/{provider}`; heartbeat reads + decrypts per-engagement via Admin SDK. |

## 5. Heartbeat (Phase E) operational reference

### 5.1 Tier II tick pipeline (per fire)

1. Read state from `<vault_root>/_memory/heartbeat-state.json` (or
   `TickState()` defaults if missing). Schema migration via
   append-only `_MIGRATIONS` dict.
2. Collect signals (`heartbeat.signals.collect.collect_signals`):
   - **Calendar** — primary calendar, next-N-hours window via
     `calendar.events.list`. Uses OAuth from
     `/etc/ikrs-heartbeat/google-token.json`.
   - **Gmail** — search `(is:unread OR is:starred) after:<cutoff>`,
     last-N-hours, capped at 25 threads/tick. Same OAuth.
   - **Vault** — recursive walk of `vault_root`, mtime diff vs
     `state.last_vault_mtimes`. Hard symlink guard (post-E.3 fix);
     ignore set: `.git`, `.obsidian`, `_memory`, `node_modules`,
     etc.; hard cap of 200 changed files in prompt rendering.
   - Errors per collector fold into `bundle.errors` — never raises.
3. Render prompt (`heartbeat.prompts.render_tick_prompt`) using
   `heartbeat/src/heartbeat/prompts/tick_prompt_v1.txt`. Threads
   `last_action_summaries` (natural-language one-liners, NOT opaque
   IDs) for dedupe context.
4. Call LLM (`heartbeat.llm.gemini.GeminiClient`) with
   `response_mime_type="application/json"` and
   `response_json_schema=TICK_RESPONSE_SCHEMA`. Currently
   `gemini-2.5-pro`. ~3000 tokens/tick (steady state).
5. Parse JSON → typed actions (`KanbanTaskAction`,
   `MemoryUpdateAction`, `TelegramPushAction`). Re-key IDs to fresh
   UUIDs server-side. Stamp `emitted_at`.
6. Save state atomically via mkstemp + fsync + os.replace.
7. Dispatch (`heartbeat.outputs.dispatch.dispatch_outputs`):
   - **KanbanTask** → `ikrs_tasks/{action.id}` (Firestore, namespaced
     to NOT collide with Mission Control's `tasks/`). Doc shape
     mirrors Tauri `Task` type (`src/types/index.ts:99`); priority
     mapped {urgent→p1, high→p2, medium→p3, low→p3}; status =
     "backlog"; source = "claude". `createdAt`/`updatedAt` are
     Python datetime → Firestore Timestamp (NOT ISO string — that
     was the post-E.5 BLOCK fix at commit 5c018dd).
   - **MemoryUpdate** → JSONL append to audit log only (no Firestore).
   - **TelegramPush** → POST to Telegram Bot API. Urgency emoji
     prefix (ℹ️ / ⚠️ / 🚨). 401 → `telegram_auth_failed`, 429 →
     `rate_limited`, etc.
   - **Telemetry** → `heartbeat_health/{tick_id}`. Tick ID derived
     from `result.tick_ts` (NOT dispatch clock) so retries overwrite
     instead of duplicating.
   - **Audit log** → 1 "kind=tick" line + 1 "kind=action" line per
     emitted action. Lines truncated to <PIPE_BUF (4096 B) for
     atomic-append guarantee on POSIX. Dedupe by action.id.
8. Return `TickResult` to systemd (rc=0 on ok/no-op, rc=1 on error).

Skipped when `result.status == "error"`: action dispatch (avoids
emitting half-baked output). Telemetry + audit still write so the
operator can see the failure.

### 5.2 Tier I (in-Tauri) tick

1. App boot → `setup()` calls `spawn_tier_i_loop(app.handle())` in
   `src-tauri/src/heartbeat/tick.rs`.
2. Tokio task sleeps 30s (warmup, lets JS attach listener), then
   `interval = tokio::time::interval(3600s)` consumes immediate
   tick, then enters `tokio::select!` between `interval.tick()` and
   `run_now.notified()`.
3. Each tick emits `heartbeat:tier-i:tick` event with payload
   `{tick_ts, tick_count, trigger}`.
4. JS listener in `src/hooks/useHeartbeatTierI.ts` listens + ALSO
   subscribes to Firestore `heartbeat_health` (filtered by
   tenantId + engagementId). Computes `TierIVerdict`:
   - `healthy` — recent Tier II tick, no error
   - `stale` — last tick >2h ago
   - `error` — last tick reported error
   - `unknown` — no telemetry yet
5. UI: `src/components/heartbeat/HeartbeatStatusCard.tsx` renders
   pill + last-tick info + "Run now" button.
6. "Run now" button → `invoke("heartbeat_run_now")` → notifies the
   `Notify` signal → Rust loop wakes via tokio::select! → fires
   immediately with trigger="manual".

### 5.3 Telegram integration

**Why it exists.** Tier II runs 24/7. When something genuinely urgent
shows up while the operator is away from their desk (asleep, on a
flight, in a meeting), email/Slack/Firestore aren't enough — the
operator needs a **lock-screen mobile push** they can't miss. Telegram
is the cheapest, most universal mobile push channel that doesn't
require us to ship a native iOS/Android app.

The intent is *narrow*: only the most urgency-flagged actions Gemini
emits become Telegram pushes. The bulk of what the heartbeat surfaces
(routine bank notifications, calendar items, vault changes) goes to
Firestore and the Kanban — visible when the operator opens the app,
silent until then. Telegram is reserved for things like:
- A client escalation that landed in the inbox at 3am.
- A same-day deadline conflict the model spots on the calendar.
- A signed-contract email that needs an immediate reply before it
  expires.

**Architecture: per-operator bot, NOT a shared bot.** Every operator
spins up their own Telegram bot via @BotFather. The bot token is
captured at install time and stored in `/etc/ikrs-heartbeat/secrets.env`
on the VM. There is no central IKAROS-controlled bot. This is a
deliberate security choice: a shared bot token in any single secrets
file would let any operator (or attacker who got the token) impersonate
the heartbeat across every operator using that bot. Per-operator bots
mean compromise is contained to one VM.

The challenge agent on Phase E v3 specifically called out a shared-bot
design as a mass-impersonation risk; v4 onward enforces per-operator.

**Setup ritual** (one-time, ~90 seconds, captured by `heartbeat/scripts/install.sh`):

1. Operator opens Telegram → DMs `@BotFather` → `/newbot` → picks a
   name + username for the bot (e.g. `IKAROS Heartbeat (Moe)` /
   `@moe_ikrs_heartbeat_bot`). BotFather replies with a token like
   `8791762466:AAEgNmc1hVLPNm8UqyRJe8UmY5Yoxt3AgW8`.
2. Operator messages their new bot once (any text — `/start` works).
   This populates the bot's update buffer with at least one chat.
3. Operator visits `https://api.telegram.org/bot<TOKEN>/getUpdates`
   in a browser. The JSON response includes
   `"chat":{"id":<NUMBER>,...}`. That `<NUMBER>` is the operator's
   chat ID. Positive integer for personal DMs (~9-10 digits);
   negative for group chats.
4. `install.sh` prompts for `TELEGRAM_BOT_TOKEN` + `TELEGRAM_CHAT_ID`,
   writes them to `/etc/ikrs-heartbeat/secrets.env` (mode 0600
   `ikrs:ikrs`), then **runs `deleteWebhook` against the bot** —
   this clears any pre-existing webhook config (the token might
   have been used elsewhere previously) so `getUpdates` works
   reliably going forward.
5. Smoke test: `sudo systemctl start ikrs-heartbeat.service`. If
   Gemini emits a `telegram_push` action, the operator gets a
   notification within seconds.

**Push pipeline.** When Gemini emits a `TelegramPushAction`, the
dispatcher calls `heartbeat.outputs.telegram.send_telegram_push`,
which:
1. Reads `TELEGRAM_BOT_TOKEN` + `TELEGRAM_CHAT_ID` from `OutputSecrets`.
2. Composes message text with an urgency-emoji prefix:
   - `urgency: "info"` → `ℹ️ <message>` (informational, low-noise)
   - `urgency: "warning"` → `⚠️ <message>` (worth attention soon)
   - `urgency: "urgent"` → `🚨 <message>` (act now)
3. POSTs to `https://api.telegram.org/bot<TOKEN>/sendMessage` with
   `chat_id`, `text`, and `disable_web_page_preview: true`. 10-second
   timeout per request.
4. Maps non-2xx responses to typed `TelegramError.error_code`:
   - HTTP 401 → `telegram_auth_failed` (token revoked, BotFather rotated)
   - HTTP 429 → `rate_limited` (Telegram throttled this bot)
   - other 4xx/5xx → `api_call_failed`
   - `requests.RequestException` → `network_error`
5. The error is recorded in the audit log + heartbeat_health doc;
   the tick still completes successfully (Telegram failure is never
   tick-fatal).

**`TelegramPushAction` schema** (Python dataclass, `heartbeat/src/heartbeat/actions.py`):
```python
@dataclass(frozen=True)
class TelegramPushAction:
    type: Literal["telegram_push"]
    id: str           # server-side UUID, set at re-key time
    message: str      # model-authored text, prefixed with emoji at dispatch
    urgency: Literal["info", "warning", "urgent"]
    emitted_at: str   # ISO-8601, set at tick time
```

The model receives the schema as part of the prompt's
`response_json_schema`. The prompt template (`tick_prompt_v1.txt`)
explicitly tells it:

> Reserve `urgency: "urgent"` for things that genuinely cannot wait
> until morning — a same-day deadline, a client escalation, a calendar
> conflict that needs to be resolved before a meeting starts. Inflated
> urgency trains the operator to ignore you.

**Tier coordination.** Telegram pushes are **Tier II only**. Tier I
(in-Tauri) does NOT send Telegram messages — its reconciler is
verification-only and surfaces issues via the in-app status pill. This
keeps the design rule "if the human is at their desk, the app surfaces
it; if they're not, Telegram surfaces it" clean.

**Operational notes**:
- Token rotation: `@BotFather` → `/revoke` → pick the bot → new token.
  Edit `/etc/ikrs-heartbeat/secrets.env`, `systemctl restart`. Old
  token immediately invalid (Telegram rejects with 401).
- Chat ID can be a **group chat** (negative integer): the operator
  can have a private "IKAROS notifications" group with multiple
  family members or a co-worker, and the bot pushes to all of them.
- The operator can mute the bot client-side (Telegram → bot DM →
  Mute). The heartbeat keeps sending; the operator just doesn't get
  woken up. Useful for sleep / vacations.
- `deleteWebhook` is run only at install time, not per-tick. Calling
  it on every tick would race with anything else using the bot.

**What the integration does NOT do (yet)**:
- No bi-directional flow. The bot is push-only. The operator can't
  reply to the bot to confirm/dismiss/snooze actions. Phase F+ will
  add `/confirm`, `/snooze 1h`, `/dismiss <id>` commands that mutate
  Firestore.
- No media (images, files, voice). Plain-text body only.
- No Telegram-native rich formatting (HTML/Markdown). Just emoji
  prefix + the model's text. Could enable `parse_mode: "MarkdownV2"`
  in a future minor release; not done yet to avoid escaping headaches.
- No quiet hours. Spec is explicit: 24/7 operation. Operator mutes
  the bot client-side if needed.
- No batching. Each `TelegramPushAction` becomes one HTTP POST.
  At one tick per hour the volume is trivial; if a future tick
  emits 10+ urgent pushes simultaneously, we'd hit Telegram's 30/sec
  per-bot limit — but the prompt's bias-to-no-op constraint makes
  this scenario unlikely in practice.

**Future work (Phase F+)**:
- Bi-directional Telegram: bot listens via `getUpdates` polling or
  webhook, parses commands, mutates Firestore (e.g.
  `/confirm <action_id>` → set `dispatchStatus: "confirmed"` on the
  action's audit row).
- Multi-engagement: today's single bot pushes everything. With
  Phase F's per-engagement Firestore-synced tokens we could route
  per-engagement pushes through different bots (or use chat ID
  parameterised by engagement so one bot handles all).
- Rich previews: include a deep-link to the Mac app's
  `ikrs-workspace://action/<id>` URI so tapping the notification
  opens the corresponding Kanban card.

### 5.4 Operational runbooks

**Build Mac app from source**:
```bash
cd ~/projects/apps/ikrs-workspace
git checkout main && git pull origin main
npm install
npm run tauri build
rm -rf "/Applications/IKAROS Workspace.app"
cp -R "src-tauri/target/release/bundle/macos/IKAROS Workspace.app" /Applications/
open "/Applications/IKAROS Workspace.app"
```

**Install Tier II on a fresh VM**:
```bash
# On the VM:
git clone https://github.com/IKAROSgit/ikrs-workspace.git ~/projects/apps/ikrs-workspace
cd ~/projects/apps/ikrs-workspace
sudo bash heartbeat/scripts/install.sh
# Answer interactive prompts; copy SA + token files separately.
```

**Deploy a code update to the VM**:
```bash
# On the VM (already installed):
cd ~/projects/apps/ikrs-workspace
git pull origin main
sudo /opt/ikrs-heartbeat/venv/bin/pip install -e ~/projects/apps/ikrs-workspace/heartbeat --quiet
sudo systemctl restart ikrs-heartbeat.service
```

**Rotate Gemini API key**:
```bash
# On the VM:
sudo nano /etc/ikrs-heartbeat/secrets.env  # update GEMINI_API_KEY
sudo systemctl restart ikrs-heartbeat.service
```

**Rotate Telegram bot token** (BotFather `/revoke` → new token):
Same as above with `TELEGRAM_BOT_TOKEN`.

**Rotate Firebase service account**:
1. Generate new key in Firebase Console → Service accounts.
2. `scp` to VM, install at `/etc/ikrs-heartbeat/firebase-sa.json`
   with mode 0600 ikrs:ikrs.
3. `sudo systemctl restart ikrs-heartbeat.service`.

**Re-bootstrap OAuth (if Google account changes or token revoked)**:

*Phase F (preferred)*: Re-connect Google in the Tauri app's Settings tab for
the affected engagement. The token is automatically encrypted and synced to
Firestore. The heartbeat picks it up on the next tick — no scp required.

*Phase E fallback (legacy, if Phase F not yet deployed)*:
```bash
# On Mac:
cd ~/projects/apps/ikrs-workspace/heartbeat
.venv/bin/python -m heartbeat.oauth_bootstrap /path/to/client_secret.json
# Browser flow, sign in with the right Google account.
scp token.json moe_ikaros_ae@elara-vm:/tmp/google-token.json
ssh moe_ikaros_ae@elara-vm
sudo install -m 0600 -o ikrs -g ikrs /tmp/google-token.json /etc/ikrs-heartbeat/google-token.json
rm /tmp/google-token.json
sudo systemctl start ikrs-heartbeat.service
```

**Migrate existing deployment to Firestore-synced tokens (Phase F)**:
```bash
# On the VM (after pulling Phase F code + pip install -e):
# 1. Ensure TOKEN_ENCRYPTION_KEY is in secrets.env
#    (install.sh generates it; or: openssl rand -base64 32)
# 2. Source secrets so the script sees the key
source /etc/ikrs-heartbeat/secrets.env
export TOKEN_ENCRYPTION_KEY TOKEN_ENCRYPTION_KEY_VERSION
export FIREBASE_SA_KEY_PATH=/etc/ikrs-heartbeat/firebase-sa.json

# 3. Dry-run first — verify what would happen
sudo -E /opt/ikrs-heartbeat/venv/bin/python \
  ~/projects/apps/ikrs-workspace/heartbeat/scripts/migrate-token-to-firestore.py \
  5L12siRpQDDXnPCk892H --dry-run

# 4. Run for real
sudo -E /opt/ikrs-heartbeat/venv/bin/python \
  ~/projects/apps/ikrs-workspace/heartbeat/scripts/migrate-token-to-firestore.py \
  5L12siRpQDDXnPCk892H

# 5. Wait for next heartbeat tick, verify it reads from Firestore
sudo systemctl start ikrs-heartbeat.service
sudo journalctl -u ikrs-heartbeat -n 50 --no-pager | grep -i firestore

# 6. Only after a successful tick, remove the legacy file
sudo rm /etc/ikrs-heartbeat/google-token.json
```

**Debug a failing tick**:
```bash
ssh moe_ikaros_ae@elara-vm
sudo systemctl status ikrs-heartbeat.service
sudo journalctl -u ikrs-heartbeat -n 200 --no-pager
sudo tail -n 5 /home/moe_ikaros_ae/vaults/<engagement>/_memory/heartbeat-log.jsonl | python3 -m json.tool
```

**Reset Tier II state (force "first run" on next tick)**:
```bash
sudo rm -f /home/moe_ikaros_ae/vaults/<engagement>/_memory/heartbeat-state.json
sudo systemctl start ikrs-heartbeat.service
```

**Deploy Firestore rules / indexes** (do this from Mac):
```bash
cd ~/projects/apps/ikrs-workspace
npx firebase-tools deploy --only firestore:rules,firestore:indexes --project ikaros-portal
```

**Rotate token encryption key (Phase F)**:
```bash
# On the VM:
NEW_KEY="$(openssl rand -base64 32)"
sudo nano /etc/ikrs-heartbeat/secrets.env
# Move TOKEN_ENCRYPTION_KEY → TOKEN_ENCRYPTION_KEY_PREV
# Move TOKEN_ENCRYPTION_KEY_VERSION → TOKEN_ENCRYPTION_KEY_PREV_VERSION
# Set TOKEN_ENCRYPTION_KEY="$NEW_KEY"
# Bump TOKEN_ENCRYPTION_KEY_VERSION (e.g. 1 → 2)
sudo systemctl restart ikrs-heartbeat.service
# Next tick reads with old key, writes back with new key (auto re-encrypt).
# After all engagements have ticked once, remove _PREV entries.
# Update Mac .env.local: VITE_TOKEN_ENCRYPTION_KEY=<new key>
```

## 6. Schema reference

### 6.1 `heartbeat_health` doc

```
{
  tenantId: string         // consultant Firebase UID
  engagementId: string     // engagements/{id} doc ID
  tier: "I" | "II"
  tickTs: string           // ISO-8601 with TZ
  status: "ok" | "error" | "skipped" | "no-op"
  durationMs: int
  tokensUsed: int
  promptVersion: string    // e.g. "tick_prompt.v1"
  actionsEmitted: int
  errorCode: string | null
  expiresAt: string        // ISO-8601, 30 days after tickTs (for TTL)
}
```

### 6.2 Heartbeat-emitted `ikrs_tasks` doc

Mirrors Tauri `Task` (src/types/index.ts:99):

```
{
  _v: 1
  id: string                        // UUID (server-generated post-E.4 fix)
  engagementId: string              // matches Tauri filter
  tenantId: string                  // denormalised for audit
  title: string
  description: string
  status: "backlog"                 // always start in backlog
  priority: "p1" | "p2" | "p3"      // mapped from urgent/high/medium/low
  tags: ["heartbeat", "tier-ii"]
  subtasks: []
  sortOrder: 0
  source: "claude"
  assignee: "consultant"
  rationale: string                 // why this matters now
  notesCount: 0
  createdAt: Timestamp              // Python datetime → Firestore Timestamp (post-E.5 fix)
  updatedAt: Timestamp
}
```

### 6.3 `TickState` (Python dataclass, persisted as JSON)

```python
@dataclass(frozen=True)
class TickState:
    schema_version: int = CURRENT_STATE_SCHEMA   # bumped via append-only _MIGRATIONS
    last_tick_ts: str | None = None
    last_seen_event_ids: list[str] = ...
    last_seen_thread_ids: list[str] = ...
    last_action_ids: list[str] = ...
    last_action_summaries: list[str] = ...       # natural-language, fed back to LLM
    last_vault_mtimes: dict[str, str] = ...
```

### 6.4 Prompt template

`heartbeat/src/heartbeat/prompts/tick_prompt_v1.txt`. Versioned —
bump file + `CURRENT_PROMPT_VERSION` together. Old versions kept on
disk so old `heartbeat_health.promptVersion` rows can be retraced.

## 7. Known limitations & open work

1. **Single-token Tier II OAuth — Phase F shipped.** Legacy
   single-token still supported via auto-wrap in config. Operator
   migrates via `heartbeat/scripts/migrate-token-to-firestore.py`
   (see §5.4 runbooks). New engagements use Firestore-synced tokens
   automatically after connecting Google in the Tauri app.
2. **Vault on Mac is not synced to VM by default.** First-soak
   deployments use an empty test vault on the VM; the vault collector
   reports zero changes. Real vault sync (Syncthing? rsync? gcsfuse?)
   is operator-choice.
3. **`ProtectHome=true` was wrong default in install.sh.** Fixed at
   commit `5715688` to flip to `false` when `vault_root` is under
   `/home/*`. Existing deployments need a one-line `sed`.
4. **`ikrs_tasks.createdAt` was string-typed pre-E.5-fix at commit
   `5c018dd`.** Old docs are invisible to the Tauri Kanban reader.
   Solution: delete old docs from Firestore Console.
5. **Audit log line size cap.** 4096 B (PIPE_BUF) for atomic appends.
   Lines exceeding the cap log a warning but still write; concurrent
   appenders may interleave bytes (cosmetic, not data corruption).
6. **No notarization on Mac builds.** Ad-hoc signing only. Apple ID
   distribution requires real notarization setup (deferred).
7. **Email privacy is on at GitHub for the operator's account** →
   commits authored as `IKAROSgit@users.noreply.github.com`. Don't
   regress by re-adding `moe@ikaros.ae` as an author email.
8. **`ProtectSystem=strict` requires explicit `ReadWritePaths`** for
   each writable path. `/etc/ikrs-heartbeat` is now in that list (commit
   `df68275`) so OAuth refresh-token rotation can persist back.
9. **Per-engagement vault paths in TOML** — Phase F.4 added
   `[[engagements]]` array with per-engagement `vault_root`. install.sh
   still appends a single `ReadWritePaths=<vault_root>` line — must be
   updated for multi-engagement (F.6).
10. **No quiet-hours config for Telegram pushes.** Spec is explicit:
    24/7 operation. Operator can mute the bot in Telegram client-side.
11. **No automated key rotation script.** Key rotation is a manual
    procedure (see §5.4 runbook). A future script could iterate all
    engagement `google_tokens` docs, decrypt with old key, re-encrypt
    with new key, and update Firestore — but the manual procedure
    (let the heartbeat auto-re-encrypt on next tick) works for the
    current single-operator deployment.

## 8. Update protocol — how to keep this doc honest

When you submit a change that touches:
- Architecture (new service, file move, new dep, identity flow change)
- Secret material (new env var, new keychain entry, new token type)
- Firestore (new collection, new field, rules change, index change)
- Scheduling (cron, systemd unit, tokio interval)
- Operator runbooks (install/deploy/rotate steps)
- Phase status (new phase, phase moved to shipped/blocked/deferred)
- Known limitations (new one discovered, old one fixed)

…you MUST update this doc in the same commit. CI enforces this via
`scripts/check-ecosystem-docs.sh`. Pre-commit hook also available
(install with `./scripts/install-pre-commit-hook.sh`).

If unsure whether your change qualifies — update the doc anyway. It's
better to over-document than to leave the next contributor (human or
AI) reading stale information.

When updating, follow these rules:
1. Update the relevant section, not the bottom.
2. Bump the "Last verified" check at the top of the doc by reading it
   fully and confirming each section is current.
3. If you removed a feature or limitation, MOVE it to a "Removed"
   subsection rather than just deleting — it preserves the history of
   what the system used to do.
4. Don't remove specific UIDs / engagement IDs from §2.1 unless they
   actually changed.

The doc is the canonical reference. Code is implementation detail.
