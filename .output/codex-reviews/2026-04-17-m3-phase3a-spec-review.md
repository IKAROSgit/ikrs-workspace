# Codex Review — M3 Phase 3a Signal Pipeline Design

**Spec:** `docs/specs/m3-phase3a-signal-pipeline-design.md`
**Reviewed at HEAD:** `097586b` (M3 milestone spec post-MC-close)
**Reviewer:** Codex (architectural)
**Date:** 2026-04-17

---

## Verdict: PASS (proceed to plan-writing) with two WARN items to address in Wave-1 planning

Score: 9/10. The spec honours every AD-1..AD-7 constraint from the parent milestone, contains zero narrative-generation or review-UI drift, holds no LLM calls anywhere (AD-2 preserved), and all spot-checked code citations at HEAD `097586b` are line-exact. The adversarial gate test catches both failure modes it claims to. Two WARN items: (1) a subtle timing gap in the adversarial scenario as written, (2) minor completeness gaps in the `session:start` event wiring relative to the parser signature change.

---

## 1. Structural — PASS

8 design sections + 2 cross-cutting + file-change table (22 rows) + risks (10) + 12 success criteria + Codex checkpoints + open questions. No duplication between Section 3 (aggregator) and the cross-cutting signal gate — the latter correctly refers back to the former's AD-4 filter rather than re-specifying it. The "Out of 3a" carveouts at the top (narratives to 3b, review UI to 3c) are unambiguous.

## 2. Architecture — PASS

- **No narrative generation in 3a.** Grep of the spec shows narrative words only in the "out of scope" and "consumed by 3b" statements. No Gemini, no `narrative-generate`, no hourly bucketer.
- **No review UI.** Only the disclosure page, which is explicitly "What the app captures" — a read-only manifest + 24h counts, not a review surface.
- **AD-2 preserved.** Zero LLM API calls anywhere in the Rust modules or frontend. Every signal is captured locally and written to SQLite. 3b will be where the Cloud Function routing appears.
- **AD-5 preserved.** `activity_events.payload` is explicitly counts/paths/names/timestamps only; success criterion 11 plus a grep CI rule enforces no raw content (tool_input / content) ever landing in SQLite.
- **AD-6 honoured.** `time_anchor::now_local()` is the single source of truth; a grep convention bans `chrono::Utc::now()` in the timesheet subsystem.

## 3. Security — PASS

- **Consent defaults OFF everywhere, including Moe's install** — explicit.
- **Signal gate is tight at ingest.** Section 3 rejects events at aggregator ingress (which is the only write path into `token_usage_buckets`), not after the fact. Both AD-3 (consent) and AD-4 (active_engagement) checks run before any accumulator touch.
- **File watcher is engagement-scoped.** Section 5 starts the watcher on `session:start` for the active engagement only and stops on `session:stop`. At most one watcher alive. It cannot physically observe a different engagement's vault.
- **Consent state machine** is a strict DAG; "Decline all" is terminal; `consent:state-changed` forces Rust-side cache invalidation; aggregator fails CLOSED on unknown state (risk row 5).
- **SQLite queries** index on `(engagement_id, timestamp)` and `(kind, timestamp)`. `query_activity_events` is parameterised on `engagement_id`. No obvious path to cross-engagement leak on query.

## 4. Completeness — PASS with one small gap

- Zero TBDs or "implement later" in design sections. Open Questions are execution-detail only, not strategic.
- All 12 success criteria are verifiable as stated.
- **Minor gap:** the file-change table row for `stream_parser.rs` says "thread `engagement_id` through `parse_stream` / `handle_line` / `handle_*_event`" and session_manager row says "pass `engagement_id` into `parse_stream` (line 132)", but Section 2 additionally threads it through `handle_assistant_event` without restating that the existing `claude:tool-start`, `claude:text-delta`, etc. emissions stay untouched. Not a blocker — a planner will pick it up — but worth a sentence.

## 5. Risk — PASS

10 risks named with concrete mitigations. The most-critical row (cross-engagement leak) names two independent filters (payload ID + active_engagement lock), a mandatory adversarial unit test, AND a runtime counter surfaced in Settings. Three-layer defence is appropriate for an NDA-breach class risk. DST risk, crash-window loss, persisted-scope revocation, and notify-on-git-checkout flood are all named with specific mitigations.

## 6. Spec/plan alignment — PASS

File-change table is precise enough (22 rows, each with action + description) for a subagent to enumerate waves. Section 7 gives exact SQL DDL. Section 3 gives struct shapes. Section 8 gives hook names. A planner can write `PLAN.md` tomorrow.

## 7. Implementation readiness — PASS on citations, WARN on one detail

**Citation spot-check at commit `097586b` — ALL five cited file:line references are line-exact:**

| Citation | Claim | Actual at HEAD |
|---|---|---|
| `stream_parser.rs:199` | `handle_assistant_event` fn | Line 199 `fn handle_assistant_event(` ✓ |
| `stream_parser.rs:259` | `handle_user_event` fn | Line 259 `fn handle_user_event(` ✓ |
| `stream_parser.rs:319-331` | `infer_mcp_server` body | Lines 319-331 the four MCP prefix checks ✓ |
| `stream_parser.rs:109` | `tool_name_map` declaration | Line 109 `let mut tool_name_map:` ✓ |
| `types.rs:45` | `AssistantMessage.usage` field | Line 45 `pub usage: Option<serde_json::Value>` ✓ |
| `Cargo.toml:31` | `notify = "8"` | Line 31 `notify = "8"` ✓ |
| `Cargo.toml:20` | `tauri-plugin-sql` declared | Line 20 ✓ |
| `lib.rs:49` | SQL plugin registration | Line 49 `.plugin(tauri_plugin_sql::Builder::new().build())` ✓ |
| `session_manager.rs:132` | `tokio::spawn` for `parse_stream` | Line 132 `tokio::spawn(async move {` ✓ |
| `session_manager.rs:168` | after `sessions.lock().await.insert(...)` | Line 168 `self.sessions.lock().await.insert(session_id.clone(), session);` ✓ |
| `session_manager.rs:224` | `monitor_process` fn | Line 224 `async fn monitor_process(` ✓ |
| `session_manager.rs:251` | before existing `claude:session-ended` emission | Line 251 `let _ = app.emit(` (matches "emit `session:stop` before this") ✓ |

Every cited line is within 0 of actual. No WARN on citations.

**WARN (implementation-readiness detail):** Section 6 says "`session:start` fires after the registry insert (never before)" and cites line 168. But `spawn()` also spawns the `monitor_process` task at line 158 BEFORE the insert at line 168. If Claude CLI is already dead by the time line 168 runs (extremely unlikely but possible), `monitor_process` could emit `session:stop` before `session:start`. The spec's success criterion 6 asserts ordering but doesn't specify what happens if the child exits between spawn() line 118 (Command::spawn) and line 168 (insert). Recommend planner adds a defensive check: emit `session:start` immediately after `Command::spawn` succeeds but before spawning the monitor task, to guarantee ordering.

---

## (A) Adversarial test scenario — PASS with a timing-gap WARN

Walking through the t=10:07:00 → t=10:07:30 case:

1. t=10:07:00 `session:stop` fires. The listener that updates `active_engagement` sets it to `None`.
2. t=10:07:30 late-A token event arrives. Aggregator reads `active_engagement == None` → rejects (fails the AD-4 gate, condition 3 assertion).

**This works correctly** because the gate is written as `*active_engagement.lock() != Some(engagement_id)` — `None != Some("A")` evaluates TRUE, so the event is rejected. Condition 3 asserts `cross_engagement_rejections = 1` post-rejection. Correct.

The t=10:11:00 rogue-A case also works: `active_engagement == Some("B")`, event carries `"A"`, `Some("B") != Some("A")` → rejected, counter increments to 2. Correct.

**WARN (real race condition not covered):** the spec does not assert ordering between the `session:stop` handler updating `active_engagement` and the late-A event arriving at ingest. In production, both events flow through Tauri's app event bus which is MPMC — if the late-A event is already queued behind `session:stop` but the aggregator's listener drains `claude:token-usage` on a different task than `session:stop`, the late-A could be processed while `active_engagement` still reads `Some("A")`. Then it would be ACCEPTED into bucket A, failing condition 3 silently.

The spec mentions "two-point filter (payload + active_engagement lock)" but both points pass in this race. Recommend the planner add a secondary guard: the session_id on the event must match a currently-live session (i.e. reject if `sessions.contains_key(event.session_id) == false`). That's a third filter that catches late events from torn-down sessions regardless of the `active_engagement` race.

As written, the six assertions are sufficient to catch the two NAMED failure modes (naive payload trust, and active_engagement-presence-only). They do NOT catch the listener-ordering race. This is a WARN because the test would pass even if the race exists — the race only manifests under concurrent listener dispatch, which the unit test's synthetic event sequencing won't reproduce.

## (B) Citation accuracy — PASS

All 12 spot-checked citations are line-exact at commit `097586b`. Zero citations off by >3 lines. Zero off at all.

---

## Blockers

None. Plan writing for Phase 3a should proceed.

## Wave-1 planner pickups

1. **Session-start ordering guarantee.** Move `session:start` emission to immediately after `Command::spawn` succeeds, before `monitor_process` task spawn, so monitor can never emit stop-before-start.
2. **Third-filter gate.** Add `sessions.contains_key(event.session_id)` check to the AD-4 gate in `token_aggregator::ingest` to close the listener-ordering race. Extend the adversarial test with a concurrent-dispatch case using `tokio::join!` across the session-stop and late-token-event paths.
3. **Minor completeness:** restate in Section 2 that existing stream-parser emissions (`claude:tool-start`, `claude:text-delta`, `claude:session-ready`) keep their current signatures — `engagement_id` threads only to new emissions. Prevents a planner from over-scoping.

---

**Relevant file paths:**

- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/specs/m3-phase3a-signal-pipeline-design.md`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/specs/m3-timesheet-automation.md`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/stream_parser.rs`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/session_manager.rs`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/types.rs`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/lib.rs`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/Cargo.toml`
- `/home/moe_ikaros_ae/projects/apps/ikrs-workspace/.output/codex-reviews/2026-04-17-m3-milestone-spec-review.md`
