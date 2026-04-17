# M3: Timesheet Automation

**Status:** Draft — awaits Codex sign-off before Phase 3a spec.
**Date:** 2026-04-17
**Scope lock input:** `docs/planning/2026-04-17-m3-scope-lock.md` (Moe answered all gating questions; Codex APPROVED 8/10)
**Codex reviews:**
- Scope-lock review 2026-04-17: APPROVED WITH CONDITIONS 8/10 — C1 (client auth), C3 (token usage is new subsystem) closed by this spec; remaining six land as phase-spec decisions.
- M3 spec review pending (this document).

**Prior milestones:**
- M1 — Consultant desktop app foundation (shipped 2026-04-11).
- M2 — Embedded Claude + MCP + polish + release readiness (shipping through Phase 4c today; 4d scheduled).

---

## Goal

Eliminate the daily redundant manual overhead of timesheet administration for consultants. The consultant works normally; the app captures a deliberately narrow set of activity signals (with strict consent), composes hourly narrative entries, rolls them into a monthly consolidated timesheet, and delivers that to the client through a branded read-only review portal where the client approves or rejects the submission. Positioning is automation-of-admin, not differentiation — product bar is "the chore goes away" rather than "best-in-market AI timesheet."

## Scope

### In scope (M3)

1. **Strict-opt-in consent flow** — first-launch and per-engagement toggles; default OFF for everyone including Moe; pause/resume at any time.
2. **Activity signal pipeline** — token usage poller (new subsystem), MCP tool event bus (new), file watcher on vault (new — `notify` crate already declared, not used), session start/stop stream.
3. **Hourly narrative generator** — Gemini Flash via existing OpenClaw routing (free AI Studio tier); hourly bucketing with majority-owner-hour rule for straddling sessions; conservative generation posture with explicit "uncertain" markers and signal-citation back to source events.
4. **Monthly consolidation + review UI** — consultant-side view within IKAROS Workspace that renders the month's narrative entries, allows edit, flags low-confidence entries, allows manual addition of off-signal activity.
5. **Submission to client** — monthly submit action publishes the consolidated timesheet to the external client-facing portal (built by M4 but scaffolded by M3 via shared auth primitive — see Architectural Decision 1).
6. **Storage schema** — activity events (SQLite local, bounded retention), narrative entries (Firestore cross-device), consolidated timesheet records (Firestore with explicit approval state machine).
7. **Export primitives** — PDF timesheet + CSV timesheet export, consultant-initiated.
8. **Privacy disclosure surface** — "what the app captures" page in Settings, viewable at any time, lists every signal + storage location + retention window.

### Out of scope (explicit — belongs to M4)

- Client-facing web portal (separate Next.js deployment at `clients.ikaros.ae`). M3 ships the **shared auth primitive** (Firebase Auth invited external users) and the **submission API** (Firestore writes that the portal reads). The portal itself is M4.
- White-label theming (per-client brand config). Firestore schema in M3 includes a `brandConfig` field on `engagement` so data is ready, but M3 UI is IKAROS-branded only.
- Real-time client view of current workstation state. M3 ships monthly submission only. Live workstation view is M4.
- Manager dashboards. Folded into M4 as views over the M3 data.
- Full multi-consultant isolation. M3 remains single-consultant-per-install; M4 introduces multi-tenancy.
- Internal messaging. Deferred past M4 per scope lock; re-evaluate vs. OpenClaw / Google Chat when the time comes (Golden Rule #9).

---

## Architectural Decisions

### AD-1: Client auth is a shared Firebase Auth primitive, scaffolded in M3 (Codex C1 close)

Monthly timesheet approval (M3) and full workstation/vault view (M4) both require the same external client user to authenticate against the IKAROS Firebase project. Deferring the auth decision to M4 would force a re-architecture. Therefore M3 ships the auth primitive:

- **IAM:** Firebase Auth with `client` custom claim. Consultants invite client users via email; invite creates a Firebase user with claim `{role: "client", engagementIds: [...]}`.
- **Firestore rules:** a client user can `read` only documents where `engagementId in request.auth.token.engagementIds` AND the document is explicitly flagged `clientVisible == true`. No writes for clients in M3.
- **M3 client-side read:** the one document type M3 exposes to the client is `timesheetSubmissions/{id}` — the consolidated monthly submission, flipping `clientVisible = true` on submit.
- **M3 does not ship** a client-facing UI. It ships the data + auth primitive that M4's portal will read.
- **M4 extends** the same auth primitive to read vaults via Drive ACLs (client's Google account granted per-engagement Drive folder access) + an in-portal live view.

### AD-2: Narrative model is Gemini Flash via OpenClaw, Haiku as a future tier knob

Gemini Flash (free AI Studio tier, routed via OpenClaw `google/` prefix per CLAUDE.md Golden Rule §Model Prefix) is the M3 default for hourly narrative generation. Rationale:

- Free at projected volume (~8 narratives × 30 days × 1 consultant = 240 calls/month — well inside AI Studio's 250 RPD flash limit).
- Fast (sub-second) — matches the "chore goes away" bar.
- Hallucination mitigation is the monthly-review-by-consultant + client-approval-gate flow, not model choice. Flash is good enough.

Haiku appears as a **config knob** (`engagement.narrativeModel: 'flash' | 'haiku'`) for engagements where the consultant wants lower hallucination, accepting paid cost. Default remains `flash`. Haiku is NOT routed through OpenClaw — it would use the Anthropic API directly, inheriting the existing IKAROS billing posture (paid, acceptable for this tier).

### AD-3: Capture is strict opt-in at every layer

Defaults are OFF. Opt-in is explicit at four gates:

1. **First-launch consent flow** — user must tick boxes for each signal class (token usage / MCP tool events / file watcher / session timestamps). Can tick all or some. Cancellation is valid and leaves capture disabled globally.
2. **Per-engagement toggle** — even with global consent, each engagement defaults `captureEnabled = false`. Consultant flips on per engagement.
3. **Session-level pause/resume** — during an active session, consultant can pause capture (one click in status bar). Resumed only by explicit click.
4. **Disclosure page** — "What the app captures" in Settings enumerates every captured field, storage, retention. Readable without any action; updates live with config changes.

Implications:
- If `captureEnabled = false`, no background agents emit signals. No events land in SQLite. No narrative entries generate for that period.
- The monthly consolidation shows explicit "capture paused — no activity recorded for this period" entries where applicable. Transparent to the consultant and, on submit, to the client.

### AD-4: Token usage IS a new subsystem (Codex C3 close)

The scope lock claimed "poll the Claude CLI for token usage" as if it were a harvest of existing data. Codex verified: `src-tauri/src/claude/types.rs:45` declares an `AssistantMessage.usage` field but `src-tauri/src/claude/stream_parser.rs` never emits it to listeners. Token usage is a **new subsystem in Phase 3a**, not a poll:

- Stream parser extended to emit `claude:token-usage` events carrying `{sessionId, engagementId, timestamp, inputTokens, outputTokens, cacheReadTokens, cacheCreationTokens}`.
- Backend aggregator in a new module `src-tauri/src/timesheet/token_aggregator.rs` bins events into 15-minute wall-clock buckets per engagement.
- Frontend subscribes via `listen('claude:token-usage', ...)` in a new `useTokenUsage` hook.

### AD-5: Storage locus per signal class

| Data | Storage | Retention | Why |
|------|---------|-----------|-----|
| Raw activity events (token usage buckets, MCP tool events, file watcher events, session start/stop) | SQLite local (new table per class in existing `ikrs.db`) | 90 days rolling | High volume, low per-event value; local keeps privacy posture high and offline capture possible. |
| Hourly narrative entries | Firestore `engagements/{id}/narratives/{hourStart}` | Indefinite while engagement active | Cross-device sync, consultant edits from any Mac. |
| Consolidated monthly timesheet submissions | Firestore `timesheetSubmissions/{submissionId}` | Indefinite (legal / billing retention) | Shared with client portal in M4, audit trail. |
| Vault markdown (per ADR-013 Phase 4d) | Drive-synced filesystem, unchanged by M3 | Per engagement lifecycle | Existing model. M3 does not touch vault content. |

### AD-6: Monthly cycle is calendar-month-by-default, per-engagement override

M3 ships calendar-month billing (1st–end of month, consultant's local timezone) as default. Engagement record gains `billingPeriod: { kind: 'calendar-month' } | { kind: 'custom', startDate, lengthDays }` to support clients on different cycles without re-architecting. Custom cycles are M3.3d polish, not 3a.

---

## Phase Decomposition

### Phase 3a — Activity Signal Pipeline + Consent Flow

**Goal:** everything a signal needs to exist, get captured, and respect consent. No narrative generation yet.

**Scope (in):**
- Consent flow UI (first-launch modal + Settings page + per-engagement toggle + status-bar pause/resume).
- Stream parser extension to emit `claude:token-usage`.
- Token aggregator (15-minute bucketing).
- MCP tool event bus — new `src-tauri/src/claude/mcp_events.rs` that the existing stream parser feeds (tool call start, tool call end, tool name, engagement).
- File watcher — wire up the already-declared `notify = "8"` crate; watch the active engagement vault; emit `vault:file-change` events.
- Session start/stop events — existing session registry gains an event emitter.
- SQLite schema for activity events, migration script, CRUD Tauri commands.
- "What the app captures" Settings page.

**Out of 3a, into 3b:** narrative generation, Gemini Flash routing.
**Out of 3a, into 3c:** review UI beyond the Settings page.

**Expected size:** comparable to M2 Phase 3b (598-line plan). 4 waves.

### Phase 3b — Narrative Generation

**Goal:** hourly narratives from the signal stream.

**Scope (in):**
- Hourly bucketer — reads SQLite activity events, groups into wall-clock hours, applies majority-owner rule for straddlers.
- Flash client — routed via existing OpenClaw/AI Studio setup (verify the VM-side gateway can be called from a desktop-app client; likely the app calls Gemini directly using the existing `GEMINI_API_KEY`, with OpenClaw used only on server side).
- Prompt template — conservative, signal-citation, explicit-uncertainty. Output schema is `{ narrative: string, confidence: 'high' | 'medium' | 'low', sourceEvents: eventId[] }`.
- Firestore write of narrative entries.
- Backfill for engagement-has-consent-but-no-narratives-yet case.
- Unit tests for bucketing edge cases + prompt output parsing.

**Expected size:** mid-size phase. 3 waves.

### Phase 3c — Consolidation + Review UI

**Goal:** consultant-facing monthly view + edit flow + submission action.

**Scope (in):**
- New view `TimesheetView` — month picker, narrative entries list per day, inline edit, low-confidence flag.
- Consultant can add manual entries (off-signal activity).
- Submission action — freezes narratives for the month, writes `timesheetSubmission` doc with `clientVisible = true`, generates submission ID.
- Invite flow — consultant enters client email addresses to invite for a given engagement; backend creates Firebase user invite with `{role: client, engagementIds: [engagementId]}` claim.
- Post-submission lock — the month's narratives become immutable once submitted.

**Expected size:** mid-to-large (UI heavy). 3 waves.

### Phase 3d — Export + Billing-Period Polish

**Goal:** consultant gets PDF/CSV + custom billing periods.

**Scope (in):**
- PDF export via a headless Chromium render (`playwright` already in dev deps).
- CSV export with standard time-tracker columns.
- Custom billing period config per engagement.
- Retention defaults (90-day SQLite purge) + export-before-purge safety.
- M3 closing Codex review.

**Expected size:** small. 2 waves.

---

## Data Model (new entities)

```
Engagement (existing, extended)
├── captureEnabled: boolean (default false)
├── capturedSignals: { tokenUsage: bool, mcpEvents: bool, vaultFiles: bool, sessionTimes: bool }
├── billingPeriod: { kind: 'calendar-month' } | { kind: 'custom', startDate, lengthDays }
├── narrativeModel: 'flash' | 'haiku' (default 'flash')
└── brandConfig: { primaryColor?, logo?, clientName? }   // scaffolded for M4

ActivityEvent (new, SQLite local)
├── id, engagementId, kind, timestamp
├── payload (JSON — shape varies by kind)
└── sessionId?

TokenUsageBucket (new, SQLite local — derived from ActivityEvent)
├── engagementId, bucketStart (ISO wall-clock, 15-min aligned)
├── inputTokens, outputTokens, cacheReadTokens, cacheCreationTokens
└── sessionCount

NarrativeEntry (new, Firestore)
├── engagementId, hourStart (ISO wall-clock, hour-aligned)
├── narrative: string
├── confidence: 'high' | 'medium' | 'low'
├── sourceEventIds: string[]
├── editedByConsultant: boolean
└── frozen: boolean (true after month submission)

TimesheetSubmission (new, Firestore)
├── id, consultantId, engagementId, clientId
├── periodStart, periodEnd
├── narrativeIds: string[] (frozen references)
├── totalHours, state: 'draft' | 'submitted' | 'approved' | 'rejected' | 'revised'
├── clientVisible: boolean
├── submittedAt, reviewedAt?, reviewerId?
└── reviewNotes?: string
```

## Cross-Cutting Concerns

- **Firestore security rules** — extend `firestore.rules` to: (a) reject client writes to anything, (b) allow client reads only where `resource.data.clientVisible == true` AND `resource.data.engagementId in request.auth.token.engagementIds`, (c) reject client access to the consultant's `activities` / raw event data even if it lands in Firestore (which per AD-5 it shouldn't).
- **Offline-first** — capture must survive network loss. Local SQLite writes are durable; Firestore sync retries on reconnect.
- **Audit log** — every client-facing data flow (submission, approval, rejection) produces an immutable audit record. Prevents later disputes about "I never saw that submission."
- **Copy + i18n posture** — M3 product copy is English-only, IKAROS-branded. M4 white-label kicks in the per-engagement brand config — M3 just schema-scaffolds it.

## Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| **AI narrative hallucinates billable activity** | Legal / reputational: invoice against fabricated work | Conservative prompt, signal-citation required, confidence tier, monthly consultant review + client approval gate. Narratives flagged `confidence: low` are rendered distinctly in the review UI. |
| **UAE PDPL / client NDA exposure in captured content** | Legal: captured content (file names, email subjects, Drive file titles, MCP tool args) may be client-confidential | Per-engagement isolation at SQLite + Firestore layer; client portal sees only their engagement; redactable export. No captured content leaves the consultant's machine uninvited. |
| **Capture fatigue — consultant disables everything and invalidates the value prop** | Product failure | Value-first consent copy (what you get, not just what you give up). In-product demos. Per-signal toggles (opt in to token usage but not file watcher, for instance). |
| **Gemini Flash hits rate limit mid-day** | Narrative gaps | 250 RPD flash is ample for ~8/day/consultant; at multi-consultant scale, fall back to Haiku on rate-limit response. |
| **Firebase Auth invited-user onboarding friction for clients** | Adoption: client gives up before approving the first timesheet | In-portal first-login wizard; invite email includes a one-click magic link; support for Google SSO. Detailed UX polish belongs in M4 portal phase. |
| **Month-boundary sessions / timezone weirdness** | Double-counted or dropped hours | Majority-owner-hour rule documented in spec AD + unit-tested; timezone stored per consultant, per engagement, per timesheet submission. |
| **OpenClaw routing drift** | Narratives unexpectedly hit paid Vertex instead of free AI Studio | Keep `google/` prefix discipline (CLAUDE.md Model Prefix Rules); M3 prompt tests assert the call path. |
| **Consultant edits a post-submission narrative** | Dispute with client over what was approved | Narratives `frozen = true` after submission; edits allowed only via "revise and re-submit" flow that creates a new submission version. |

## Success Criteria

1. A consultant completing first-launch can get through consent flow, enable capture for one engagement, and see their first activity event land in SQLite inside the same session.
2. Token usage is observable per engagement at 15-minute granularity — proof: open Settings → "What the app captures" and see a live count increment during a Claude session.
3. Narratives generate within 60 seconds of the end of an hour bucket for an active engagement; Flash routing confirmed via network inspector + AI Studio dashboard (no Vertex traffic).
4. Monthly consolidation renders: all narratives for the month, edit-in-place works, manual entry supported.
5. Submission action produces a `timesheetSubmission` doc; a Firebase-Auth invited client user can read it (rules test passes) and cannot read anything else (rules test passes).
6. PDF + CSV exports match the consolidated data.
7. All phase-level Codex reviews PASS.
8. Milestone closing review: no FAILs on security, completeness, spec/code alignment.

## Open Phase-Level Questions (Decided in Phase Specs, Not Here)

1. Exact SQLite schema column types + indexes for query patterns.
2. Consent flow modal copy — needs legal review for PDPL compliance (Phase 3a).
3. Hourly bucket timezone anchor — consultant-local vs. UTC vs. per-engagement-configured (Phase 3b).
4. Flash prompt template — will iterate during 3b implementation.
5. Review UI layout — table vs. timeline vs. calendar view (Phase 3c).
6. Export PDF template design — use the ui-ux-pro-max skill when on Mac for the visual polish (Phase 3d).
7. Custom billing period UX — can a client mid-engagement switch from calendar-month to custom without disrupting prior submissions? (Phase 3d).
8. Retention — 90 days is a guess; revisit based on actual usage patterns (Phase 3d).

---

## Codex Conditions From Scope Lock — Status

From `.output/codex-reviews/2026-04-17-m3-scope-lock-review.md`:

| # | Finding | Status in this spec |
|---|---------|---------------------|
| C1 | Client auth model cannot wait for M4 | **Closed** — AD-1 ships the auth primitive in M3; portal UI is M4. |
| C2 | ADR-013 contradiction (line-manager vs. client) | **Closed by Phase 4d spec amendment** today. |
| C3 | Stream parser usage subsystem | **Closed** — AD-4 names it as a new subsystem; Phase 3a ships it. |
| C4 | Parallel-drafting M3 + 3a risks rework | **Closed** — sequencing changed; 3a spec waits for this M3 spec to pass Codex. |
| C5 | Q5 (client portal surface) is strategic | **Closed** — AD-1 locks External Web Portal (Moe's pick: B). |
| C6 | Q4 (Flash vs Haiku) affects hallucination claim | **Closed** — AD-2 locks Flash default + Haiku knob (Moe's pick: Flash). |
| C7 | Flash vs Haiku hallucination posture | **Closed** — AD-2 + Risks table enumerate narrative-conservative design (prompt, confidence, review gate). |

All Codex conditions on the scope lock are addressed. This spec is the input to Phase 3a spec writing.

## What Happens After This Spec Ships

1. Codex reviews this M3 milestone spec. Target: PASS 9+/10.
2. If PASS: I draft Phase 3a spec (activity pipeline + consent) using this spec as parent.
3. Codex reviews Phase 3a spec.
4. If PASS: Phase 3a plan-writing, then execution with wave-based parallelization and checkpoint reviews.
5. Phases 3b / 3c / 3d follow the same cadence.

Moe's orthogonal actions (not blocking this flow):
- Clone the now-public repo onto Mac, run `./tools/scripts/local-ad-hoc-sign.sh install` to use the app daily.
- Upload `TAURI_SIGNING_PRIVATE_KEY` + `_PASSWORD` to GitHub Secrets before any `v*` tag.
- Apple Developer enrolment on Apple's timeline.
- Phase 4d vault migration when at Mac with Drive signed in.
