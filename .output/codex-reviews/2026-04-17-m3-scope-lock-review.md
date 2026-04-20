CODEX REVIEW
============
Subject: M3 Scope Lock (2026-04-17)
Type: design-review (scope lock, gates spec-writing)
Date: 2026-04-17
Reviewed by: Codex
Reviewer email: moe@ikaros.ae

Input: `docs/planning/2026-04-17-m3-scope-lock.md`
Supersedes prior review: `.output/codex-reviews/2026-04-17-m3-scoping-review.md` (7/10)

VERDICTS
--------
1. Structural:     WARN — 3a→3b→3c→3d still partitions cleanly under the admin-chore reframe, but the reframe *does* shift work into 3a that the brainstorm implicitly parked in 3b/3c: (a) consent flow (first-launch + per-engagement toggle), (b) a consultant-facing "what was recorded" transparency log (precondition for honest monthly submission), (c) edit-history/audit trail for the raw signal log (needed because 3c editing must be defensible when the client reviews). Add these to 3a explicitly. Also the monthly-cycle mid-month gap handling (capture paused / edited / machine off) is a 3a data-model concern, not a 3d export concern — the raw event log needs explicit `paused`, `gap`, `edited` states from day one.

2. Architecture:   WARN — two real issues. (i) The client-facing surface split between M3 (read-only timesheet) and M4 (portal + vault/workstation view) is underspecified at the cut line. Scope lock §Reviewer model lists four client capabilities (workstation state, vault files, timesheet, monthly approve) but §What Happens Next implies only monthly-approve lands in M3. Pick one: either M3 ships *just* the monthly submit-and-approve loop (recommended — auth scaffolding + one read-only view + one write op), or M3 needs the portal scaffold too and the scope doubles. Be explicit. (ii) White-label per-engagement is currently M4, but `engagement.brandConfig` in Firestore and a theming contract (CSS custom-property tokens beat dynamic Tailwind class composition — Tailwind's JIT cannot safely do runtime-unknown classes without a safelist explosion) should be *scaffolded* in M3's Firestore schema to avoid an expensive migration when M4 starts. One schema field, one theme token layer — cheap now, expensive later.

3. Security:       WARN — three gaps. (a) **Consent UX as friction wall:** strict opt-in is correct, but if the first-launch consent dialog is modal + multi-screen + legalese, even Moe will click through it and the honesty goal is lost. Design the consent flow as one screen with plain-language bullet points and a single "enable capture for this engagement" toggle — not a ToS wall. (b) **Client-facing auth model is undefined.** Scope lock defers it to M4, but "client can view workstation state" (M4) and "client can approve monthly timesheet" (M3) both need the same auth primitive. If M3 ships monthly approval without deciding whether the client is a Firebase Auth user, an email+magic-link recipient, or a Drive-shared-doc reader, you will re-architect in M4. Decide the auth model *now* even if only one capability ships in M3. (c) **Per-engagement data isolation with external reads.** Firestore rules today are deny-all + per-collection allowlist; adding external client principals means either (i) Firebase Auth with a `clients/{clientId}` collection and custom claims scoping reads to `engagement.clientId == auth.clientId`, or (ii) no Firebase access for clients at all and the client surface is a server-rendered view. Pick before spec.

4. Completeness:   WARN — of the 8 open questions listed (lines 94-101), two are strategic-disguised-as-detail and should go back to Moe now, not wait for spec phase: **Q5 (client portal surface: in-app invited user vs external web vs Drive-native)** — this decision drives M3's auth model *and* M4's deployment target *and* whether ikrs-workspace gains a second deployable artifact. Not a spec detail. **Q4 (Flash vs Haiku for narrative)** — scope lock §Codex-conditions acknowledges hallucination risk, and model choice materially affects that risk (Haiku's instruction-following on "do not invent" is measurably stronger than Flash on long-context, per public evals); deferring this leaves the risk-mitigation design incomplete. Q1-Q3, Q6-Q8 are genuine spec-phase questions.

5. Risk register:  WARN — hallucination risk is acknowledged but the mitigation in the scope lock ("consultant reviews + edits before client sees, narrative marked 'uncertain', citations to source signals") is a *design intent*, not a mechanism. Two concrete items must land in the M3 spec: (a) a **never-invent contract** in the narrative prompt — if the token-usage signal for a given hour is zero and no other signal fires, the narrative for that hour must be a literal "no recorded activity" string, not a generated one; (b) a **source-signal citation requirement** where every narrative sentence in the monthly timesheet links back to the raw event(s) it summarises, so the consultant's edit UI shows the receipts. Also new: **monthly-cycle resilience risk** — if capture was off for 3 weeks of a month, the monthly timesheet can either (i) refuse to generate, (ii) generate only for captured days with gaps shown, or (iii) let the consultant backfill manually. Scope lock doesn't pick. Recommend (ii) with an explicit gap-rendering contract; (iii) reintroduces the hallucination risk via human backfill.

6. Spec alignment: WARN — two contradictions with existing specs worth flagging before spec-writing: (i) **ADR-013 / Phase 4d spec says "line-manager" visibility via Drive ACLs** (`docs/specs/m2-phase4d-vault-migration-design.md:13,31`), but the scope lock pivots the reviewer to external *client*. Drive ACLs to an external client email is architecturally viable but different from line-manager ACLs (different principals, different revocation story, client-NDA implications on what else lives in that Shared Drive folder). Either amend Phase 4d spec or note in M3 spec that the reviewer pivot changes the Phase 4d ACL target. (ii) Scope lock claims "token counts ≠ content" (line 28) as the privacy justification, which is true, but **the current `stream_parser.rs` does not emit usage at all** — it parses `claude:text-delta`, `claude:tool-start`, `claude:tool-end`, `claude:turn-complete` (with `cost_usd`/`duration_ms`), but the `AssistantMessage.usage` field at `src-tauri/src/claude/types.rs:45` is deserialized into a generic `serde_json::Value` and never surfaced through any event. So Phase 3a's first sub-task is *not* "poll token usage" — it is "extend stream parser to emit a `claude:usage` event carrying input/output/cache token counts per turn, plus a session-level aggregator that buckets into 15-min windows." This is a new subsystem, not a harvest. The scope lock must say so.

7. Readiness:      WARN — scope lock is substantially tighter than the brainstorm and addresses 6/6 prior conditions, but for spec-writing to start cleanly three items must close: (R1) pick the client auth model (Firebase Auth user vs magic-link vs Drive-ACL) at the decision level, even if only monthly-approve ships in M3; (R2) answer the Flash-vs-Haiku question for M3.3b so the hallucination mitigation is designable; (R3) declare the stream-parser token-emission subsystem as a named 3a deliverable (not a "poll the CLI" one-liner). On sequencing: **drafting M3 spec and Phase 3a spec in parallel is the wrong order.** Lock the M3 milestone spec first (scope, data contracts, reviewer cut, storage locus, auth model), then 3a spec derives its signal/storage/consent shape from it. In parallel risks 3a making data-shape choices that the M3 spec then has to accommodate or reverse.

DECISION: APPROVED WITH CONDITIONS
Score: 8/10

Conditions (all must close before spec-writing begins):
C1. Decide client-facing auth model now (Firebase Auth user + custom claims is the recommended default; magic-link or Drive-ACL are the alternates). Even if M3 ships only monthly-approve, the auth primitive is shared with M4.
C2. Pick Flash vs Haiku for the 3b narrative generator, or explicitly declare "Flash v1, Haiku as quality upgrade gated on hallucination rate > X%" with X named.
C3. Rename Phase 3a's first deliverable from "poll token usage" to "extend stream parser with `claude:usage` event + 15-min aggregator" and acknowledge it as a new subsystem (the usage field exists on the JSON but is not emitted).
C4. Add to 3a scope: consent flow (one-screen plain-language), per-engagement capture toggle with paused/active state machine, and a consultant-visible "what was recorded" transparency log.
C5. Pick monthly-cycle partial-data policy (recommend: generate with explicit gap rendering, no AI-backfill of uncaptured days).
C6. Scaffold `engagement.brandConfig` in the M3 Firestore schema even though white-labelling is M4 — CSS custom-property tokens, not dynamic Tailwind class composition.
C7. Sequence: **lock M3 milestone spec first, then draft 3a spec** — not parallel.
C8. Amend Phase 4d spec (`docs/specs/m2-phase4d-vault-migration-design.md:13,31`) to reflect the reviewer pivot from line-manager to external client, or note in M3 spec that 4d's ACL target changes.

Decision for Moe
----------------
Scope lock is close to spec-ready but not there yet — close C1 (client auth model) and C3 (stream-parser usage subsystem is new work, not a harvest) before opening the M3 spec doc; the other six can land inside the spec as explicit decisions. Do not start 3a spec in parallel — sequence it after the M3 milestone spec locks.

Relevant files
--------------
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/planning/2026-04-17-m3-scope-lock.md
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/.output/codex-reviews/2026-04-17-m3-scoping-review.md
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/stream_parser.rs (handle_assistant_event drops usage field)
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/types.rs (line 45: AssistantMessage.usage deserialized but never emitted)
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/specs/m2-phase4d-vault-migration-design.md (lines 13, 31: assumes line-manager, not external client)
- /home/moe_ikaros_ae/ikaros-platform/CODEX.md
- /home/moe_ikaros_ae/ikaros-platform/CLAUDE.md (Golden Rules #11, #12)
