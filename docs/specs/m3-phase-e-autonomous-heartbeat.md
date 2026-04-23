# M3 Phase E — Autonomous Heartbeat (v5: Dual-Tier, ToS-Safe, Action)

**Status:** v5 LOCKED 2026-04-23. No more revisions before code.
**Supersedes:** v1–v4 — all blocked by challenge-agents for legitimate reasons (over-engineering, scheduler fiction, shared-bot, ToS, blast radius). Moe's 2026-04-23 dual-tier insight resolves all four v4 showstoppers in one move.
**Mode:** SHIP. Skip pre-code challenge on v5 (fixes derive directly from prior agents' findings — no new architectural surface to attack). Post-code challenge per sub-phase still applies.

## Architecture (final)

**Two tiers, split by human presence:**

| Tier | When | Where | Model | Role |
|---|---|---|---|---|
| **Tier I — Claude** | App is open (human present) | Inside Tauri app, tokio interval every hour while open | Claude Code (already authenticated) | Verify, enhance, vet, act — high-quality reasoning, human-supervisable. Anthropic ToS-clean (human present, not unattended automation). |
| **Tier II — Gemini** | App is closed (human absent) | On Moe's elara-vm, Python systemd timer every hour, 24/7 | Gemini paid tier (Google AI Studio) | Analytics, data learning, research, "consultant's offline number 2". Non-human-use permitted on Gemini paid tier. |

Both write to Firestore. Tauri app's existing Firestore listeners pick up both tiers' updates seamlessly. Continuous coverage with the right tool for each context.

## Why this is the right shape (and v1–v4 were not)

- **v1**: in-Tauri tokio + LaunchAgent + Anthropic API tier — first-run wizard, TOS-gray wrapping
- **v2**: Claude-native scheduled tasks — conflated three Claude scheduling products
- **v3**: v2 + Telegram + challenge gate — shared-bot impersonation, inherited scheduler fiction
- **v4**: Pure VM + claude-cli for commercial — TOS violation under Consumer Terms; SA blast radius; OAuth handwave
- **v5**: Tier I (Claude in-app, human-present) + Tier II (Gemini on-VM, autonomous). Each tier uses its native, ToS-permitted, sanctioned mechanism. No conflict.

## Hard constraints (still all hold)

1. Mac-side install zero-touch beyond Google login (Tier I just works once user signs in)
2. ToS-compliant on both sides (separation by tier achieves this)
3. No IKAROS-held API keys (Moe's Gemini is his own AI Studio account; commercial tenants bring their own)
4. Self-healing across reinstall/update (Tier I auto-runs on app boot; Tier II self-restarts via systemd)
5. Commercial template (clone-and-go to new VMs; future commercial extensions in Phase F)

## Repo layout

```
ikrs-workspace/                          (existing private repo)
├── [existing Tauri app]
├── src-tauri/src/heartbeat/             (NEW — Tier I, Rust, in-app)
│   ├── mod.rs
│   ├── tick.rs                          (hourly tokio interval, calls Claude session)
│   └── reconciler.rs                    (verifies Tier II's actions on app open)
├── heartbeat/                           (NEW — Tier II, Python, on-VM)
│   ├── README.md
│   ├── pyproject.toml
│   ├── src/heartbeat/
│   │   ├── main.py
│   │   ├── tick.py
│   │   ├── config.py
│   │   ├── llm/gemini.py
│   │   ├── signals/{calendar,gmail,vault}.py
│   │   ├── outputs/{firestore,telegram,audit}.py
│   │   └── telemetry.py
│   ├── tests/
│   ├── systemd/{ikrs-heartbeat.service,ikrs-heartbeat.timer}
│   ├── scripts/{install.sh,uninstall.sh,smoke-test.sh}
│   └── config/heartbeat.toml.example
└── docs/specs/m3-phase-e-autonomous-heartbeat.md   (this file)
```

## Tier I: in-app Claude (while user is present)

- **Trigger**: `tokio::time::interval(3600s)` spawned in Tauri `setup()`, fires on hour boundaries while the app is open. Misses while app is closed (Tier II covers).
- **Mechanism**: invokes the engagement's existing Claude Code subprocess (already authenticated as the user). No new auth, no new API.
- **Role**: high-quality verification of Tier II's actions (read recent `heartbeat_health` rows, sanity-check the agent's writes, escalate anything questionable to a UI banner). Also handles user-initiated "Run now" button.
- **Output**: writes to `_memory/heartbeat-log.jsonl` and `pending-notifications.jsonl` exactly like Tier II — same schema, single audit trail.

## Tier II: on-VM Gemini (when user is absent or always)

- **Trigger**: systemd timer `ikrs-heartbeat.timer`, hourly.
- **Mechanism**: Python service invokes `google-generativeai` with `GEMINI_API_KEY` (Moe's paid AI Studio account; commercial tenants bring their own).
- **Role**: signal collection (Gmail/Calendar/vault diff), prompt the LLM, write structured actions to Firestore (Kanban tasks, memory updates), telemetry doc, optional Telegram push.
- **Auth on VM**: one-time installed-app OAuth flow during `install.sh` for Gmail/Calendar; service-account JSON for Firestore. Both stored in `/etc/ikrs-heartbeat/secrets.env` with 0600 perms.
- **Vault access**: `vault_root` config'd per-engagement; consultant chooses the path.

## Telegram

Per-user bot via BotFather (one-time, 90 seconds):
- Install script auto-runs `deleteWebhook` (handles the case where the token was previously webhooked by anyone else — Telegram's `getUpdates` is incompatible with active webhooks)
- Detects empty updates and re-prompts user to message the bot first
- Saves bot_token + chat_id into `secrets.env`
- Direct POST to Telegram API on urgency-flagged ticks

## Telemetry

`heartbeat_health` Firestore collection, one doc per tick:
```json
{
  "tenantId": "moe-ikaros-ae",
  "engagementId": "blr-world",
  "tier": "II",  // or "I" for app-side
  "tickTs": "2026-04-23T20:00:00+04:00",
  "status": "ok",  // ok | error | skipped | no-op
  "durationMs": 8420,
  "tokensUsed": 3200,
  "promptVersion": "tick_prompt.v1",
  "actionsEmitted": 1,
  "errorCode": null,
  "expiresAt": "2026-05-23T20:00:00+04:00"  // 30-day TTL
}
```

Firestore rules block (drop into existing rules file in same commit as E.5):
```
match /heartbeat_health/{docId} {
  allow read: if request.auth != null;  // any authed user reads — viewer pattern
  allow write: if false;  // only admin SDK (service account) writes
}
```

## Sub-phases (~3.5 days)

| # | Scope | Duration |
|---|---|---|
| E.1 | Repo bootstrap: `heartbeat/` folder, `pyproject.toml`, stubs, CI Python lane | 0.25d |
| E.2 | LlmClient (Gemini adapter only — Claude adapter deferred until needed) | 0.5d |
| E.3 | Signal collectors (calendar, gmail, vault, last-tick state with atomic writes + schema_version) | 0.75d |
| E.4 | Tick orchestrator + prompt template v1 | 0.5d |
| E.5 | Outputs: Firestore (with rules update for `heartbeat_health`) + Telegram (with `deleteWebhook` bootstrap) + Telemetry | 0.5d |
| E.6 | systemd units + `install.sh` (idempotent, secrets-preserving, OAuth flow) + `uninstall.sh` + `smoke-test.sh` | 0.5d |
| E.7 | Tier I in-app: `src-tauri/src/heartbeat/` Rust module, hourly tokio interval while app open, reads + verifies Tier II's writes | 0.5d |
| E.8 | Settings UI tab + status pill, deploy to Moe's elara-vm, first 24h soak | 0.5d |

Each sub-phase: code → post-code challenge-agent → smoke test → merge. Pre-code challenge skipped on v5 since fixes are derived from prior agents' findings — no new architectural surface to attack.

## Risks (revisited under v5)

| Risk | Mitigation |
|---|---|
| Gemini paid quota exceeded | Skip LLM call on no-op ticks; telemetry tracks `tokensUsed`; alert at 80% of monthly quota. |
| OAuth on VM expires | Refresh-token-aware client. Telemetry records `error_code = "oauth_refresh_failed"`. Settings tab shows "VM OAuth needs renewal" banner. |
| Tier I and Tier II race (both fire close together) | `heartbeat_health` row carries `tier`; if Tier II wrote a doc within last 30 min, Tier I no-ops the write side and only does verification. |
| systemd timer drift on VM reboots | `OnBootSec=10min` + `OnUnitActiveSec=1h` + `Persistent=true` — catches missed runs after reboot. |
| Telegram bot token rotated | Detect 401 from Telegram, telemetry `error_code = "telegram_auth_failed"`, skip this tick's push. |
| `heartbeat_health` doc write fails | Tick logs error to local JSONL, retries next tick. No infinite-retry loop. |

## Out of scope (Phase F or later)

- claude-cli adapter on tenant VM (Anthropic ToS — never)
- claude-api adapter (deferred until first commercial tenant onboards with their own ANTHROPIC_API_KEY)
- Per-tenant Firebase projects (deferred — Moe accepts single-project posture for now)
- Cloud Function proxy for Firestore writes (deferred — same)
- Bi-directional Telegram (Phase F)
- Commercial onboarding wizard (Phase F)
- Ops dashboard (Phase F — for now, Firebase console is operator's view)

## Action: starting E.1 immediately after this commit
