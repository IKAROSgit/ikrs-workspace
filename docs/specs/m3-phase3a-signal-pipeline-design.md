# M3 Phase 3a: Activity Signal Pipeline + Consent Flow

**Status:** Draft
**Date:** 2026-04-17
**Parent spec:** `m3-timesheet-automation.md` (M3 milestone)
**Constraints:** AD-1 through AD-7 from parent; in particular AD-3 (strict opt-in), AD-4 (token usage is a new subsystem + per-engagement signal gate), AD-5 (raw events SQLite-local only, never Firestore), AD-6 (consultant-local wall-clock timezone anchor).
**Prior phases:** M2 Phase 4a (sandbox signing, binary resolver, persisted-scope), M2 Phase 3b (MCP wiring, `tool_name_map` in `stream_parser.rs`).
**Out of 3a, into 3b:** narrative generation, `narrative-generate` Cloud Function wiring, Gemini Flash routing, hourly bucketer.
**Out of 3a, into 3c:** review UI beyond the "What the app captures" Settings page.
**Codex reviews:** pending (Phase 3a spec review).

---

## Goal

Stand up every moving part a signal needs to exist, be captured, respect consent, and land in durable local storage — without writing a single narrative. After 3a, a consultant who has opted in can start a Claude session on engagement A, see token-usage buckets, MCP tool events, vault file changes, and session start/stop events accumulate in SQLite tagged for engagement A only, pause capture mid-session from the status bar, open "What the app captures" in Settings and see live 24h counts, and quit the app with zero cross-engagement contamination. Phase 3b consumes these events; 3a produces them.

## Scope

### In scope

1. **Consent flow UI** — first-launch modal, Settings disclosure + per-engagement toggles, status-bar pause/resume.
2. **Stream parser extension** — emit `claude:token-usage` from `AssistantMessage.usage`.
3. **Token aggregator** — 15-minute wall-clock buckets per engagement, persisted to SQLite.
4. **MCP tool event bus** — surface tool-call-start/end as `mcp:tool-event` Tauri events.
5. **File watcher** — wire `notify = "8"`, watch active engagement vault, debounce 500ms.
6. **Session start/stop events** — extend `session_manager.rs` to emit `session:start` / `session:stop`.
7. **SQLite schema + CRUD** — activity-events table, forward-only migration, three Tauri commands.
8. **"What the App Captures" Settings page** — static manifest + live 24h counts.

Plus two cross-cutting: **per-engagement signal gate** (AD-4 MC-4) and **timezone anchor helper** (AD-6 MC-3).

### Out of scope

- Narrative generation / `narrative-generate` Cloud Function — Phase 3b.
- Hourly bucketer and majority-owner-hour rule — Phase 3b.
- Monthly consolidation view, edit flow, submission action — Phase 3c.
- PDF/CSV export, custom billing periods, 90-day retention purge UX — Phase 3d.
- Firestore rules for narratives/submissions — Phase 3b / 3c.
- Firebase Auth `invite-client-user` Cloud Function — Phase 3c.
- Raw captured content ever leaving the consultant's machine — never (AD-5).

---

## Design

### 1. Consent Flow

Per AD-3 the four gates are independent — all four must open for a signal to land in SQLite. Defaults are OFF everywhere, including Moe's own install.

**Gate 1: First-launch modal** (new component `src/components/consent/FirstLaunchConsentModal.tsx`). Rendered from `App.tsx` when `consentStore.firstLaunchCompleted === false`. Four unchecked toggles (token usage / MCP tool events / vault file changes / session timestamps) + plain-language copy per signal. Two buttons: "Enable selected" and "Decline all". Both mark `firstLaunchCompleted = true`; "Decline all" leaves every signal OFF globally. Cannot be dismissed by overlay click — explicit decision required.

**Gate 2: Per-engagement toggle** (new section in `EngagementSettingsView`). Even with global consent, each engagement defaults `captureEnabled = false`. Flipping it on writes to Firestore `engagements/{id}` per AD-4 schema (`captureEnabled`, `capturedSignals: { tokenUsage, mcpEvents, vaultFiles, sessionTimes }`). The per-signal booleans default to the global consent values but can be narrowed per-engagement.

**Gate 3: Session-level pause/resume** (new `src/components/status-bar/CaptureIndicator.tsx`). While a Claude session is active, a small chip shows "Capturing" (green) or "Paused" (grey). Click toggles `sessionStore.capturePaused`. The aggregator (Section 3) reads this state before accepting any event; on pause, in-flight events are dropped, not queued.

**Gate 4: Disclosure page** (Section 8). Always readable from Settings without toggling anything.

**State machine (frontend, `src/stores/consentStore.ts`):**

```
Uninitialised
  ├─(first-launch modal shown)→ AwaitingFirstLaunch
  │    ├─(Decline all)→ GloballyDisabled (terminal)
  │    └─(Enable selected)→ GloballyEnabled { signals: {...} }
GloballyEnabled
  ├─(engagement toggle ON + session start)→ PerSessionActive
  │    └─(pause click)→ PerSessionPaused ──(resume)→ PerSessionActive
  └─(engagement toggle OFF)→ PerSessionInactive
```

Transitions emit `consent:state-changed` so the Rust aggregator can invalidate its in-memory cache.

**Firestore shape** (extends existing `engagements/{id}`):

```ts
{
  captureEnabled: boolean,                 // AD-3 gate 2
  capturedSignals: {
    tokenUsage: boolean,
    mcpEvents: boolean,
    vaultFiles: boolean,
    sessionTimes: boolean,
  },
  // narrativeModel, billingPeriod, brandConfig — scaffolded by M3 parent, not touched in 3a
}
```

### 2. Stream Parser Extension

`src-tauri/src/claude/stream_parser.rs:199` (`handle_assistant_event`) already receives the full assistant message. Currently it iterates `content` blocks but ignores `raw["message"]["usage"]`. `AssistantMessage.usage` at `src-tauri/src/claude/types.rs:45` exists but is never emitted.

**Change:** after the content-block loop, read `raw["message"]["usage"]`. When present, emit:

```rust
let _ = app.emit(
    "claude:token-usage",
    TokenUsagePayload {
        session_id: raw["session_id"].as_str().unwrap_or("").to_string(),
        engagement_id: /* resolved from session context — see below */,
        timestamp: time_anchor::now_local().to_rfc3339(),
        input_tokens:          usage["input_tokens"].as_u64().unwrap_or(0),
        output_tokens:         usage["output_tokens"].as_u64().unwrap_or(0),
        cache_read_tokens:     usage["cache_read_input_tokens"].as_u64().unwrap_or(0),
        cache_creation_tokens: usage["cache_creation_input_tokens"].as_u64().unwrap_or(0),
    },
);
```

**Engagement resolution:** stream parser currently has no engagement context. Signatures for `parse_stream` and `handle_line` extend with `engagement_id: String`, threaded from `session_manager.rs:132` (`tokio::spawn(async move { parse_stream(stdout, engagement_id, parser_app).await; })`). This is the **sole source of truth for engagement tagging** per AD-4 MC-4 — the parser knows which engagement spawned its stream and every event it emits carries that ID. No ambient lookup, no "current engagement" global.

`TokenUsagePayload` added to `src-tauri/src/claude/types.rs` alongside `McpAuthErrorPayload`.

### 3. Token Aggregator

New module `src-tauri/src/timesheet/token_aggregator.rs`. New parent module `src-tauri/src/timesheet/mod.rs` declared in `lib.rs`.

**Structure:**

```rust
pub struct TokenAggregator {
    // engagement_id + bucket_start → accumulator
    buckets: Arc<Mutex<HashMap<(String, DateTime<Tz>), BucketAccumulator>>>,
    db: Arc<tauri_plugin_sql::DbInstances>, // handle for persistence
    consent: Arc<ConsentSnapshot>,          // AD-3 gate check
    active_engagement: Arc<Mutex<Option<String>>>, // AD-4 gate check
}

struct BucketAccumulator {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    session_ids: HashSet<String>,
}
```

**Bucketing algorithm:** bucket key = `time_anchor::floor_to_15min(event_timestamp)`. On event receipt:

1. **AD-3 gate check:** if `consent.capture_enabled(engagement_id) == false` or `session_paused(engagement_id) == true` → drop (log `debug!`, no write).
2. **AD-4 gate check:** if `*active_engagement.lock() != Some(engagement_id)` → drop + log `warn!("cross-engagement event rejected: active={:?} event={}", active, event_eng)` + increment a `cross_engagement_rejections` counter surfaced in Settings.
3. Lookup-or-insert accumulator keyed by `(engagement_id, bucket_start)`.
4. Add token counts; insert session_id.
5. Debounced flush: on every event, schedule a `tokio::time::sleep(Duration::from_secs(30))` then `flush_bucket(key)` unless a later event resets the timer. On bucket-boundary crossing (new event falls in a new bucket), immediately flush the previous bucket.

**Persistence:** `flush_bucket` writes via `tauri-plugin-sql` INSERT-OR-REPLACE into the `token_usage_buckets` table (Section 7). Bucket rows are idempotent — re-running flush with the same key overwrites.

**Listener wiring:** `lib.rs` setup hook creates a `TokenAggregator`, spawns a `tokio::spawn` that calls `app.listen("claude:token-usage", ...)` and forwards payloads to `aggregator.ingest(payload)`.

### 4. MCP Tool Event Bus

New module `src-tauri/src/claude/mcp_events.rs`. Surfaces MCP tool lifecycle as a separate event stream. The stream parser already identifies MCP tools via the `mcp__<server>__<tool>` prefix (`stream_parser.rs:319-331`, `infer_mcp_server`) and keeps a `tool_name_map` (line 109) so tool-end events can be correlated back to the name that started them — this was the Phase 3c fix in commit `26dbb71`.

**Event shape** (`mcp:tool-event`):

```rust
struct McpToolEventPayload {
    session_id: String,
    engagement_id: String,
    timestamp: String,          // RFC3339 consultant-local
    phase: String,              // "start" | "end"
    tool_id: String,            // Claude's block id
    tool_name: String,          // e.g. "mcp__gmail__read_message"
    server: String,             // gmail | calendar | drive | obsidian
    success: Option<bool>,      // end only
}
```

**Integration points:**

- `handle_assistant_event` (`stream_parser.rs:199`): when a `tool_use` block arrives and `tool_name.starts_with("mcp__")`, emit a `phase: "start"` event in addition to the existing `claude:tool-start`.
- `handle_user_event` (`stream_parser.rs:259`): when a `tool_result` arrives and `tool_name_map[tool_id].starts_with("mcp__")`, emit a `phase: "end"` event with `success = !is_error`.
- `mcp_events.rs` owns the payload struct + an `emit_mcp_tool_event` helper; the parser calls the helper so the shape has a single home.

A listener in `token_aggregator.rs`'s sibling dispatcher forwards these into SQLite as `kind = "mcp-tool-event"` rows (Section 7), gated by the same AD-3 + AD-4 filters.

### 5. File Watcher

New module `src-tauri/src/timesheet/vault_watcher.rs` using the already-declared `notify = "8"` (`src-tauri/Cargo.toml:31`, zero current usage).

**Lifecycle:** started on `session:start` for the active engagement, stopped on `session:stop`. At most one watcher alive at a time — single-consultant, single-session assumption carried over from M2.

**Path resolution:** `vault_path = ~/.ikrs-workspace/vaults/{client_slug}` via existing Phase 4d path resolver if/when ADR-013 lands in that phase; otherwise the default used throughout M2 (`skills/mod.rs:7` comment confirms). 3a takes whichever resolver is present at implementation time — no fork.

**Debounce:** `notify::RecommendedWatcher` feeds a `tokio::sync::mpsc::channel`. A dedicated task drains the channel with a 500ms trailing-edge debounce window — i.e. an Obsidian save that rewrites a markdown file + sidecar attachments + `.obsidian/workspace.json` in the same ~100ms fires ONE `vault:file-change` event carrying the union of affected paths, not three.

**Event shape** (`vault:file-change`):

```rust
struct VaultFileChangePayload {
    engagement_id: String,
    timestamp: String,
    paths: Vec<String>,    // relative to vault root
    kinds: Vec<String>,    // create | modify | remove (per-path)
}
```

Noise filters: ignore paths under `.obsidian/`, `.trash/`, and `.git/` (workspace auto-saves aren't billable signal).

### 6. Session Start/Stop Events

`session_manager.rs` already has a registry and a monitor task (`monitor_process` at line 224). It emits `claude:session-ended` on exit but no explicit `claude:session-started` — there's a `session-ready` from the stream parser's system-init path, which is *system* ready, not *registry* started.

**Extension points:**

- `spawn()` (line 31): after `self.sessions.lock().await.insert(...)` (line 168), emit `session:start` with `{engagement_id, session_id, started_at: time_anchor::now_local().to_rfc3339(), pid: child_pid}`.
- `monitor_process` (line 224): before the existing `claude:session-ended` emission (line 251), emit `session:stop` with `{engagement_id, session_id, stopped_at, reason, exit_code}`. Do NOT rewrite `claude:session-ended` — it's consumed by UI already.
- `active_engagement` tracking: aggregator's `active_engagement: Arc<Mutex<Option<String>>>` is updated from a dedicated listener on `session:start` / `session:stop`. This is what AD-4 MC-4's gate in Section 3 reads.

No new struct; two `app.emit` calls plus a channel to the aggregator. Zero risk of disturbing the existing session lifecycle because the new events are purely additive.

### 7. SQLite Schema + CRUD

`tauri-plugin-sql` is already registered (`lib.rs:49`, `Cargo.toml:20`) but has no migrations and no callers yet. 3a adds the first migration.

**DB file:** `{app_data_dir}/activity.db` — the Tauri plugin convention. `app_data_dir` on macOS is `~/Library/Application Support/ae.ikaros.workspace/` post-Phase-4a.

**Schema (forward-only additive migration `001_activity_events.sql`):**

```sql
CREATE TABLE IF NOT EXISTS activity_events (
    id              TEXT PRIMARY KEY,           -- uuid v4
    engagement_id   TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN (
                       'token-usage-bucket',
                       'mcp-tool-event',
                       'vault-file-change',
                       'session-event'
                     )),
    timestamp       TEXT NOT NULL,              -- RFC3339 consultant-local
    session_id      TEXT,
    payload         TEXT NOT NULL,              -- JSON, schema varies by kind
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX IF NOT EXISTS idx_activity_eng_ts
    ON activity_events(engagement_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_activity_kind_ts
    ON activity_events(kind, timestamp);

CREATE TABLE IF NOT EXISTS token_usage_buckets (
    engagement_id         TEXT NOT NULL,
    bucket_start          TEXT NOT NULL,        -- RFC3339, 15-min aligned, consultant-local
    input_tokens          INTEGER NOT NULL DEFAULT 0,
    output_tokens         INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens     INTEGER NOT NULL DEFAULT 0,
    cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
    session_count         INTEGER NOT NULL DEFAULT 0,
    updated_at            TEXT NOT NULL,
    PRIMARY KEY (engagement_id, bucket_start)
);
```

Migrations registered via `tauri_plugin_sql::Builder::default().add_migrations("sqlite:activity.db", vec![Migration { version: 1, ... }]).build()` replacing the current `::new().build()` at `lib.rs:49`.

**Tauri commands** (new module `src-tauri/src/timesheet/commands.rs`):

```rust
#[tauri::command]
pub async fn record_activity_event(
    engagement_id: String,
    kind: String,
    timestamp: String,
    session_id: Option<String>,
    payload: serde_json::Value,
    state: State<'_, TokenAggregator>, // for AD-4 gate consultation
    db: State<'_, DbInstances>,
) -> Result<String, String>;

#[tauri::command]
pub async fn query_activity_events(
    engagement_id: String,
    from: String,
    to: String,
    db: State<'_, DbInstances>,
) -> Result<Vec<ActivityEventRow>, String>;

#[tauri::command]
pub async fn purge_old_events(
    older_than_days: u32,
    db: State<'_, DbInstances>,
) -> Result<u64, String>; // returns rows deleted
```

Registered in `lib.rs` `invoke_handler` alongside existing `claude::commands::*`. `purge_old_events` is exposed for 3d's 90-day retention job but callable manually in 3a.

### 8. "What the App Captures" Settings Page

New view `src/views/CaptureDisclosureView.tsx` routed from Settings. Two sections:

**Static manifest** (hard-coded in `src/lib/capture-manifest.ts`) listing every signal + storage + retention:

| Signal | Source | Stored | Retention |
|--------|--------|--------|-----------|
| Token usage (15-min buckets) | Claude CLI stream | SQLite local | 90 days |
| MCP tool events | Stream parser | SQLite local | 90 days |
| Vault file changes | `notify` watcher | SQLite local | 90 days |
| Session start/stop | Session manager | SQLite local | 90 days |

**Live 24h counts:** `useActivityCounts()` hook calls `query_activity_events(activeEngagementId, now-24h, now)`, groups by `kind`, renders count badges. Updates on a 30s interval + on every `consent:state-changed`. Zero external dependencies — reads SQLite only, no Firestore, no network.

Page includes a standing "Pause all capture now" button mapped to Gate 3, and a "Purge older than N days" control bound to `purge_old_events`.

---

## Cross-cutting concerns

### Per-engagement signal gate (AD-4 MC-4)

This is the single most important correctness property in M3 — violating it leaks one client's data into another client's narrative, and in UAE PDPL terms that's an NDA breach. The gate lives in **Section 3's AD-4 filter** at ingest time.

**Adversarial test scenario** (Rust unit test in `token_aggregator.rs::tests`, must land with 3a):

```
t=10:00:00   session:start  → engagement A, session_A
t=10:03:00   claude:token-usage { engagement: A, tokens: 100 }   ACCEPT → bucket A/10:00
t=10:05:00   claude:token-usage { engagement: A, tokens: 200 }   ACCEPT → bucket A/10:00
t=10:07:00   session:stop   → session_A (user killed without switching UI)
t=10:07:30   claude:token-usage { engagement: A, tokens:  50 }   (late event from buffered stream)
t=10:08:00   session:start  → engagement B, session_B
t=10:10:00   claude:token-usage { engagement: B, tokens: 300 }   ACCEPT → bucket B/10:00
t=10:11:00   claude:token-usage { engagement: A, tokens: 999 }   (rogue event — parser bug, tagged wrong)
t=10:12:00   claude:token-usage { engagement: B, tokens: 400 }   ACCEPT → bucket B/10:00
```

**"Correct" looks like:**

1. Bucket `(A, 10:00)` = 300 input tokens (the two t=10:03 and t=10:05 events only).
2. Bucket `(B, 10:00)` = 700 input tokens (the two t=10:10 and t=10:12 events only).
3. The t=10:07:30 late-A event is REJECTED — `active_engagement = None` at that moment (between session_A stop and session_B start). Counter `cross_engagement_rejections` = 1.
4. The t=10:11:00 rogue-A event is REJECTED — `active_engagement = B` but event carries `A`. Counter `cross_engagement_rejections` = 2.
5. Zero rows in `token_usage_buckets` where `engagement_id = 'A'` have `bucket_start = '10:00'` and `input_tokens` > 300.
6. Zero rows in `token_usage_buckets` where `engagement_id = 'B'` have `input_tokens` containing the 999 value or the late-A 50.

The test asserts all six conditions. A naive implementation that trusts payload tagging alone fails conditions 3 and 4; an implementation that gates only on `active_engagement` presence fails condition 3 less obviously. The spec calls out both failure modes so the test is wired to catch them.

### Timezone anchor (AD-6 MC-3)

New helper `src-tauri/src/timesheet/time_anchor.rs` — single source of truth for "what hour does this timestamp belong to?" Prevents scattered `chrono` calls with inconsistent TZ handling.

**Signature:**

```rust
use chrono::{DateTime, TimeZone};
use chrono_tz::Tz;
use std::sync::OnceLock;

static SYSTEM_TZ: OnceLock<Tz> = OnceLock::new();

pub fn init() {
    // Called once from lib.rs setup hook. Reads iana-time-zone crate or
    // `/etc/localtime` → IANA string → chrono_tz::Tz. Caches for process lifetime.
}

pub fn system_tz() -> Tz { *SYSTEM_TZ.get().expect("time_anchor::init not called") }

pub fn now_local() -> DateTime<Tz>;
pub fn floor_to_15min(dt: DateTime<Tz>) -> DateTime<Tz>;
pub fn floor_to_hour(dt: DateTime<Tz>) -> DateTime<Tz>; // used by 3b, declared here
pub fn iana_name() -> String; // e.g. "Asia/Dubai", stored on TimesheetSubmission per AD-6
```

Every timestamp written to SQLite in 3a passes through `now_local().to_rfc3339()`. No raw `chrono::Utc::now()` calls anywhere in the timesheet subsystem. A clippy-level convention (enforced by grep in CI, not a custom lint) keeps the discipline.

---

## File change summary

| File | Action | Description |
|------|--------|-------------|
| `src-tauri/src/claude/stream_parser.rs` | MODIFY | Emit `claude:token-usage` from `AssistantMessage.usage`; emit `mcp:tool-event` on MCP tool start/end; thread `engagement_id` through `parse_stream` / `handle_line` / `handle_*_event` |
| `src-tauri/src/claude/types.rs` | MODIFY | Add `TokenUsagePayload`, `McpToolEventPayload`, `SessionStartPayload`, `SessionStopPayload` |
| `src-tauri/src/claude/session_manager.rs` | MODIFY | Emit `session:start` after registry insert (line 168); emit `session:stop` in `monitor_process` (line 224); pass `engagement_id` into `parse_stream` (line 132) |
| `src-tauri/src/claude/mcp_events.rs` | CREATE | `emit_mcp_tool_event` helper; payload definitions |
| `src-tauri/src/timesheet/mod.rs` | CREATE | Parent module declaration |
| `src-tauri/src/timesheet/token_aggregator.rs` | CREATE | `TokenAggregator` struct, 15-min bucketing, AD-3 + AD-4 gates, adversarial unit test |
| `src-tauri/src/timesheet/vault_watcher.rs` | CREATE | `notify`-backed watcher, 500ms debounce, noise filters |
| `src-tauri/src/timesheet/time_anchor.rs` | CREATE | System TZ cache + floor helpers |
| `src-tauri/src/timesheet/commands.rs` | CREATE | `record_activity_event`, `query_activity_events`, `purge_old_events` |
| `src-tauri/migrations/001_activity_events.sql` | CREATE | Forward-only migration for `activity_events` + `token_usage_buckets` |
| `src-tauri/src/lib.rs` | MODIFY | `mod timesheet;`, register migration with `tauri_plugin_sql`, `time_anchor::init()`, `.manage(TokenAggregator::new(...))`, register 3 new commands in `invoke_handler` |
| `src-tauri/Cargo.toml` | MODIFY | Add `chrono-tz = "0.9"`, `iana-time-zone = "0.1"` |
| `src/stores/consentStore.ts` | CREATE | First-launch + per-signal state machine, `consent:state-changed` emission |
| `src/components/consent/FirstLaunchConsentModal.tsx` | CREATE | Gate 1 modal |
| `src/components/status-bar/CaptureIndicator.tsx` | CREATE | Gate 3 pause/resume chip |
| `src/views/CaptureDisclosureView.tsx` | CREATE | "What the app captures" page |
| `src/views/EngagementSettingsView.tsx` | MODIFY | Add per-engagement capture toggles (Gate 2) |
| `src/lib/capture-manifest.ts` | CREATE | Static signal manifest |
| `src/hooks/useActivityCounts.ts` | CREATE | 24h count query hook |
| `src/lib/tauri-commands.ts` | MODIFY | Add wrappers for `record_activity_event`, `query_activity_events`, `purge_old_events` |
| `src/types/activity.ts` | CREATE | Shared payload + row types |
| `src/App.tsx` | MODIFY | Mount `FirstLaunchConsentModal` when `firstLaunchCompleted === false` |

---

## Risks

| Risk | Impact | Mitigation | Status |
|------|--------|------------|--------|
| Stream parser misses `usage` on short turns | Token buckets undercount; low-activity hours look idle | Emit on every assistant message regardless of content-block presence; unit test with a usage-only message | Open |
| Aggregator in-memory buckets lost on crash | Up to 15 minutes of activity vanishes | 30-second trailing-flush to SQLite caps loss window; final flush on `session:stop`; aggregator re-hydrates unflushed buckets from SQLite on startup (idempotent INSERT-OR-REPLACE) | Open |
| Cross-engagement event slips past the AD-4 gate | NDA breach — client A's tokens attributed to client B | Two-point filter (payload `engagement_id` + `active_engagement` lock); adversarial unit test mandatory; `cross_engagement_rejections` counter surfaced in Settings for runtime visibility | Open (most-critical) |
| Notify watcher fires thousands of events on a git checkout inside the vault | UI lag, SQLite row flood | 500ms debounce + ignore `.git/`, `.obsidian/`, `.trash/`; cap `paths` array at 100 entries per event | Open |
| Consent state desynchronised between frontend store and Rust aggregator | Signals captured after user toggles off (privacy violation) | `consent:state-changed` listener in aggregator re-reads Firestore state on every transition; aggregator fails CLOSED (drops events) when state is unknown | Open |
| Timezone ambiguous across DST transitions | Hour-bucket math produces duplicate or missing buckets on 25-hour and 23-hour days | `chrono_tz` handles DST natively; use `.earliest()` disambiguation and document behaviour in `time_anchor.rs` tests | Open |
| SQLite migration fails on upgrade from pre-3a install | App fails to start, activity capture unavailable | Forward-only additive migration (no ALTER of existing tables); `IF NOT EXISTS` guards; plugin falls back gracefully on version mismatch | Open |
| Persisted-scope revokes vault access post-Phase-4a | File watcher starts then immediately errors with EPERM | Catch watcher setup errors and emit `vault:watch-unavailable`; disclosure page surfaces "vault file changes: unavailable (workspace access required)" | Open |
| User disables all four signals but leaves `captureEnabled = true` | Empty narratives in 3b with confusing "capture paused" rendering | Frontend enforces: if all four per-signal booleans are false, `captureEnabled` auto-sets to false; UI tooltip explains the coupling | Open |
| `notify = "8"` API churn vs older Rust examples | Compilation failures mid-wave | Reference notify 8.x `Watcher::new` API directly; pin minor version in Cargo.toml to avoid surprise | Open |

---

## Success Criteria

1. First-launch modal renders on a fresh install (no `firstLaunchCompleted` flag), cannot be dismissed by overlay click, and "Decline all" leaves every signal globally OFF with zero events landing in SQLite for the next session.
2. With global consent ON + engagement A `captureEnabled = true` + session running, a Claude turn emitting `usage: { input_tokens: 100, output_tokens: 200 }` produces exactly one row in `token_usage_buckets` keyed by `(A, floor_to_15min(turn_timestamp))`.
3. Status-bar pause chip, when clicked mid-session, causes the next 5 `claude:token-usage` events to be dropped (verified via `cross_engagement_rejections`-style logged drop counter); resume re-admits events without restarting the session.
4. MCP tool events surface as `mcp:tool-event` with correct `server` inference for all four prefixes (`mcp__gmail__*`, `mcp__calendar__*`, `mcp__drive__*`, `mcp__obsidian__*`) and a row per start/end pair lands in `activity_events` with `kind = 'mcp-tool-event'`.
5. An Obsidian save that touches a markdown file + two attachments within 200ms produces exactly ONE `vault:file-change` event whose `paths` array contains all three, not three events.
6. `session:start` fires after the registry insert (never before); `session:stop` fires before `claude:session-ended`; both carry matching `engagement_id` + `session_id`.
7. `query_activity_events(engagement_A, now-1h, now)` returns only engagement-A rows, zero engagement-B rows, even after the adversarial cross-engagement scenario from Section "Per-engagement signal gate" runs as an integration test.
8. "What the app captures" page renders the static manifest within 100ms of navigation and the live 24h count badges reflect SQLite reality (verified by inserting 3 synthetic rows and watching the badge tick from N to N+3).
9. `purge_old_events(90)` removes every row with `timestamp < now - 90 days` and returns the count.
10. Adversarial cross-engagement unit test in `token_aggregator.rs::tests` passes with all six conditions from the Per-engagement Signal Gate section asserted.
11. No raw captured content (file contents, tool arguments, message text) is ever persisted — `activity_events.payload` contains only counts, paths, names, and timestamps. A code-review checklist item enforces this and a grep-based CI rule flags any `tool_input` / `content` field landing in the SQLite write path.
12. All phase-level Codex reviews PASS.

---

## Codex Checkpoints

**Ck-1 (mid-wave, after Waves 1–2):** Stream parser extension + token aggregator + session events landed, adversarial test green, SQLite migration applied cleanly on a fresh and a migrated install. Review focus: AD-4 gate correctness, engagement threading through `parse_stream`.

**Ck-2 (end of 3a, after Waves 3–4):** Consent UI + file watcher + disclosure page + all 12 success criteria verified. Review focus: gate 1–4 wiring, `consent:state-changed` propagation, no raw content in SQLite, watcher debounce behaviour under a git-checkout stress test.

---

## Open Questions (for phase execution, not strategy)

1. Should `query_activity_events` paginate? At 90-day retention × ~500 events/day, single-engagement queries cap ~45k rows — borderline. Decide during Wave 2 based on actual UI consumer shape.
2. `notify = "8"` on macOS uses `FSEvents`; does it fire on `.obsidian/workspace.json` auto-saves we've filtered out, or are those already below its granularity? Empirical during Wave 3.
3. Gate 1 modal copy needs legal review for UAE PDPL compliance — surface during Wave 4.
4. If the user revokes Firestore credentials mid-session, the consent Firestore read fails. Does the aggregator fall back to last-cached state or fail CLOSED? Lean fail-closed; confirm during implementation.
5. `chrono-tz` adds ~2MB to the binary. Acceptable given Phase 4a's signed .dmg already ships ~85MB; confirm no tree-shaking alternative in Wave 1.
6. `session:stop` reason field — do we distinguish "user clicked disconnect" from "Claude CLI crashed"? Existing `classify_exit` gives us the raw exit code; decide whether to wrap or pass through.
7. Should the 500ms watcher debounce be configurable per-engagement for NDA-sensitive clients who want sub-second precision? Defer to 3d polish unless an engagement needs it.
