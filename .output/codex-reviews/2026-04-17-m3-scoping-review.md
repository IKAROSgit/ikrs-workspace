CODEX REVIEW
============
Subject: M3 scoping brainstorm (2026-04-17)
Type: design-review (pre-spec scoping)
Date: 2026-04-17
Reviewed by: Codex
Reviewer email: moe@ikaros.ae

Input: `docs/planning/2026-04-17-m3-scoping-brainstorm.md`
Protocol: CODEX.md 7-point validation; brainstorm (not spec), so verdicts gate spec-writing, not shipping.

VERDICTS
--------
1. Structural:     WARN — the (3a signals -> 3b narrative -> 3c UI -> 3d export) decomposition partitions cleanly at the layer level, but the seam between 3a and 3b is under-specified: the brainstorm never says whether the raw signal store is append-only event log, daily-bucketed aggregate, or both. That choice dictates 3b's prompt shape and 3d's retention story, so it must be nailed in the spec, not discovered during 3b. Also, no explicit phase for "migration / backfill of historical Claude sessions already on disk" — either fold into 3a or declare out of scope.
2. Architecture:   WARN — the claim "all signals already exist in M2" is 60% true. Confirmed present: `src-tauri/src/claude/stream_parser.rs` (emits tool_use / text / result blocks), `claude/session_manager.rs` + `claude/registry.rs` (session registry and per-engagement attribution), `claude/mcp_config.rs` (MCP server config). **Not present:** no file watcher is actually wired — `notify = "8"` is declared in `src-tauri/Cargo.toml` but grep finds zero `use notify` in `src-tauri/src/`, so "file writes" as a timesheet signal is a **new subsystem, not a harvest**. MCP events are modeled in `claude/types.rs` but there is no standalone MCP event emitter/bus — tool_use events are parsed out of the Claude stream, which is fine, but the brainstorm overstates it as a separate input. Calendar-block signal is entirely new (no GCal integration in ikrs-workspace today). Recommend the spec explicitly list these as 3a deliverables rather than 3a integrations.
3. Security:       WARN — privacy flag is missing. Passive activity capture of a consultant's Claude turns + file writes is surveillance-adjacent even when self-directed; brainstorm does not mention consent UX, pause/incognito mode, redaction of client PII before narrative generation, or what leaves the machine. Q4 ("manager visibility") asks the delivery question but not the data-minimization question. Storage choice (Firestore vs local SQLite vs engagement-vault markdown) is unaddressed and has ADR-013 implications: if timesheets live in the engagement vault on Drive, line-manager ACL is nearly free but deletion-right and "pause tracking" get harder. Must be resolved before spec.
4. Completeness:   WARN — candidate set is reasonable and W-deferral is defensible given X is W's value prop, but three gaps: (a) no candidate for **offline/air-gapped capture** (consultants travel — does 3a buffer locally?); (b) telemetry/analytics folded into V without scope — it is actually the observability substrate for X and deserves explicit treatment; (c) no mention of how existing M2 session history (already on disk under `~/.ikrs-workspace/`) participates on day one.
5. Risk register:  WARN — OpenClaw-as-messaging-alternative (per Golden Rule #9) is architecturally viable: `OPENCLAW_INTEGRATION.md` confirms Google Chat is already the primary comms channel and OpenClaw owns Slack/WhatsApp/Teams/Discord adapters in Phase 4. **However:** OpenClaw is server-side on elara-vm, not embedded in the Tauri app, so "messaging inside ikrs-workspace" via OpenClaw means either (i) a thin webview on the Funnel, or (ii) a new desktop<->gateway protocol. Neither is free. Deferring Z past M4 is the right call, but the brainstorm implies "just use OpenClaw" is turnkey — it is not, and that should be captured as a known risk for whenever Z is revisited. Also missing from the risk list: regulatory (UAE PDPL / client NDAs on captured work content), and AI-narrative hallucination risk (a timesheet that invents billable hours is a legal document problem, not just a UX one).
6. Spec alignment: PASS — this is a brainstorm, not a spec, and it correctly flags itself as such per Golden Rule #11 + `superpowers:brainstorming`. Naming-drift section (original-M2 == timesheets, silently became M4) is exactly the kind of honest audit the discipline exists to surface, and is reason enough to endorse X. Open questions are well-shaped.
7. Readiness:      FAIL (for spec-writing, not for decision) — if Moe approves "X -> W -> Y-folded -> defer Z" right now, we still cannot start `docs/specs/m3-timesheet-engine-design.md` cleanly. Blockers: Q1 (signal set), Q2 (granularity + billing codes), Q3 (edit model), Q4 (manager visibility mechanism), and the three items flagged in Security above (consent/pause, PII redaction, storage locus) must have answers. Q5 is a commercial question only Moe can answer. Q7 (ADR-013 Phase 4d ordering) is a genuine cross-milestone dependency — recommend deciding 4d lands **inside** M3 (probably as 3a.0) rather than after, because the timesheet store location depends on it. Q8 (billing) is safely out of scope.

DECISION: APPROVED WITH CONDITIONS (as a brainstorm; spec-writing is blocked until conditions close)
Score: 7/10

Conditions (all must close before `m3-timesheet-engine-design.md` begins):
C1. Answer Q1-Q4 + Q6-Q7 in a short addendum to this brainstorm or directly in the spec's "Decisions" section.
C2. Add a Privacy & Consent subsection to the brainstorm (or commit to one in the spec): pause/incognito, PII redaction before Claude narrative call, retention, deletion rights.
C3. Correct the "all signals already exist" claim: explicitly list file-watcher instantiation (notify crate is declared but unused) and calendar integration as **new** 3a deliverables, not harvests.
C4. Decide storage locus (vault markdown vs Firestore vs SQLite) with ADR-013 Phase 4d ordering resolved. Recommend: engagement-vault markdown for the narrative + local SQLite for the raw signal log, with Phase 4d landing as a 3a prerequisite.
C5. Add two risks to the register: AI-narrative hallucination on billable time (mitigation: consultant approval is mandatory, not auto-approve) and OpenClaw-inside-Tauri is non-trivial (note for when Z is revisited).
C6. Explicit one-liner on what happens to pre-M3 Claude session history: ignored, backfilled, or opt-in backfill.

Decision input for Moe
----------------------
Verdict: **The recommendation (X then W, Y folded into W, Z deferred with OpenClaw as the later alternative) is architecturally sound and correctly resurrects the original-M2 commercial differentiator that silently slipped** — approve the sequence, but do not let spec-writing start until C1-C6 close (mostly one short session of answering the open questions honestly). Before you pick, think hardest about two things: (1) the privacy/consent posture — passive capture of your own Claude turns is fine, passive capture once external consultants are on the app (post-W) is a different product that needs a pause-switch and PII redaction designed in from 3a, not retrofitted; (2) whether Q5 is actually still yes — if during M2 you quietly decided the curated Claude experience is itself the wedge, then X-as-timesheets is a feature, not a milestone, and W jumps the queue.

Relevant files
--------------
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/planning/2026-04-17-m3-scoping-brainstorm.md
- /home/moe_ikaros_ae/ikaros-platform/CODEX.md
- /home/moe_ikaros_ae/ikaros-platform/CLAUDE.md (Golden Rules #9, #11)
- /home/moe_ikaros_ae/ikaros-platform/.architecture/OPENCLAW_INTEGRATION.md
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/stream_parser.rs
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/registry.rs
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/session_manager.rs
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/mcp_config.rs
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/Cargo.toml (line 31: `notify = "8"` — declared, unused)
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/specs/m2-phase4d-vault-migration-design.md (ADR-013 Phase 4d, prereq candidate for M3)
