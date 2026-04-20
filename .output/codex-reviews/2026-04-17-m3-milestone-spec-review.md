CODEX REVIEW
============
Subject: M3 Milestone Spec — Timesheet Automation (`docs/specs/m3-timesheet-automation.md`)
Type: design-review (milestone spec, gates Phase 3a spec-writing)
Date: 2026-04-17
Reviewed by: Codex
Reviewer email: moe@ikaros.ae

Inputs:
- `docs/specs/m3-timesheet-automation.md`
- `docs/planning/2026-04-17-m3-scope-lock.md`
- Prior review `.output/codex-reviews/2026-04-17-m3-scope-lock-review.md` (8/10, 7 conditions)
- `docs/specs/m2-phase4d-vault-migration-design.md` (amended with "Reviewer pivot")
- `/home/moe_ikaros_ae/.openclaw/openclaw.json` (routing evidence)
- `/home/moe_ikaros_ae/ikaros-platform/.architecture/OPENCLAW_INTEGRATION.md`

VERDICTS
--------
1. Structural:     PASS — Four phases (3a signals+consent → 3b narrative → 3c review UI → 3d export+polish) partition cleanly; six ADs front-load the load-bearing decisions; data model, cross-cutting, risks, success criteria, open questions all present. 3a scope correctly absorbs consent flow, "What the app captures" page, and per-engagement toggle (closing C4 of the scope-lock review). Monthly-cycle gap semantics are correctly pushed to 3b as a data-model concern via the `captureEnabled=false → "capture paused"` rendering rule at spec lines 76–77. Only nit: 3d lumps "M3 closing Codex review" into phase scope at line 159 — reviews are ceremony, not scope; remove the bullet and keep it as a gate.

2. Architecture:   WARN — AD-1 (auth scaffolded in M3, portal in M4) holds. M3 ships exactly: custom-claim shape, Firestore rules, one writable doc type (`timesheetSubmissions`), invite flow. M4 extends the same primitive with Drive ACLs + portal UI. That's the minimum viable shared primitive; it does not over-build. **But one architectural hole:** the spec never specifies *who writes Firebase custom claims*. Custom claims require Admin SDK privilege — you cannot set them from the consultant's desktop app (that would require shipping an Admin SDK service-account key, which is a distribution-killer). This needs a callable Cloud Function (or a server-side endpoint on the command-center) that the consultant's app invokes to create/update client invites. Declare this in AD-1 or it becomes a 3c-blocking surprise. Related: spec line 145 says "backend creates Firebase user invite with `{role: client, engagementIds: [...]}` claim" — "backend" is hand-wavy; name the deployment target (Cloud Function on ikaros-portal) and put it in AD-1.

3. Security:       WARN — Three findings.
   (a) **URL-guessing is blocked, per-engagement isolation works**: the `clientVisible` boolean + `engagementId in request.auth.token.engagementIds` conjunction at spec line 51 is correctly AND-ed, so a client on engagement A cannot read engagement B's submission by guessing a doc ID even if `clientVisible=true` on it. This is sound.
   (b) **Raw activity events are in SQLite local (AD-5) — NOT Firestore — so there is no network-reachable raw-event surface for clients to guess at. Good.** But the spec's cross-cutting bullet at line 205 says "reject client access to the consultant's `activities` / raw event data *even if it lands in Firestore (which per AD-5 it shouldn't)*." This parenthetical is a latent contradiction — if raw events are SQLite-only, the rule is vacuous; if they ever spill to Firestore (audit log? telemetry?), the rule must be explicit. Recommend: harden AD-5 with "no raw activity events are ever written to Firestore under any circumstance" and remove the belt-and-suspenders Firestore rule wording, OR define the exact Firestore path and write the rule decisively.
   (c) **PDPL/NDA mitigation is underspecified at the narrative layer.** AD-1 isolates submissions per engagement. But the NarrativeEntry content itself (spec line 186–191) is AI-generated text that *will* quote file names, tool-call arguments, and email subject lines that may be client-confidential. The risk table at line 215 says "redactable export" but the data model has no redaction field, no audit of "what words did the prompt inject into output," and no consultant-pre-submit scrub step. For a consultant on multiple engagements, the failure mode is: narrative for client A's hour accidentally cites a file name from client B's concurrent session (both open in Obsidian). The majority-owner-hour rule doesn't fix this — it picks an owner, but the signal set for that hour may still include cross-engagement leaks if the consultant was multitasking. **Required fix for 3b spec:** a per-engagement signal-gate at the bucketer level — events whose `engagementId` doesn't match the bucket's owning engagement are dropped, not summarized. Call it out in AD-3 or AD-5.

4. Completeness:   WARN — Of the 8 open phase-level questions, Q3 (hourly bucket timezone anchor) is **strategic-now**, not phase-spec-later. Timezone choice determines: (a) the shape of the `NarrativeEntry.hourStart` key (UTC-aligned vs. local-aligned keys are migration-incompatible), (b) how submissions span DST transitions, (c) what a client in a different timezone sees on the portal. Recommend: lock "consultant-local wall-clock, timezone stored per submission" now in AD-6 and delete the Q3 deferral. Q7 (mid-engagement billing-period switch) is also a data-model question, not a UX question — the answer determines whether `TimesheetSubmission.periodStart/End` are derived or denormalised. Push both into AD-6 now. The other six questions are genuinely phase-spec-level.
   Also missing from the open-questions list: **what happens when a consultant leaves an engagement mid-month** — do the captured events for days-before-departure stay? Who owns the final submission? Recommend one-line answer in AD-6.

5. Risk register:  WARN — Eight risks is reasonable coverage, but three gaps to close before 3a:
   (a) **Rate-limit cascade:** risk 4 handles "Flash hits rate limit" with a Haiku fallback — but Haiku is per AD-2 a paid path outside OpenClaw. If Flash quota exhausts silently mid-month across multiple consultants (multi-tenant M4 scope, admittedly), cost jumps without a budget alert. Needs an explicit Lester-owned budget-guard at the prompt-invocation layer. Not blocking M3 (single consultant) but name it as a deferred risk for M4.
   (b) **Audit-log immutability:** the cross-cutting bullet at line 207 promises "immutable audit record" but no mechanism is specified. Firestore writes are mutable by default. Either (i) use a Firestore rule that denies updates on the audit collection outright, (ii) mirror to GCS with object-versioning + retention-lock, or (iii) both. Pick in 3c spec; flag now so the data model for `auditLog` lands in 3a.
   (c) **Consultant-in-dispute-with-client scenario:** spec risk 8 handles "consultant edits a post-submission narrative" via the `frozen=true` flag + revise-and-resubmit flow. But the inverse case is unaddressed: client rejects the submission, consultant disputes the rejection. What's the audit trail when `reviewNotes` is the only channel and the two parties disagree about billable hours? Recommend: explicit "rejection is immutable once issued; dispute is a new consultant-initiated document, not an edit of the existing submission" contract in AD-6 or a new AD.

6. Spec alignment: PASS — The conditions table at spec lines 253–259 accurately closes all seven scope-lock conditions. Spot-checked:
   - **C1 (client auth shared):** Closed — AD-1 ships custom claims + rules + one writable doc + invite flow in M3, defers portal UI to M4. Genuinely shared primitive, not double-built.
   - **C3 (token usage is a new subsystem):** Closed — AD-4 at spec lines 79–86 names the new module `src-tauri/src/timesheet/token_aggregator.rs`, the new event `claude:token-usage`, the new hook `useTokenUsage`, and correctly attributes the gap to the current `stream_parser.rs` not emitting `AssistantMessage.usage` (matches the prior review's file:line citation at `src-tauri/src/claude/types.rs:45`).
   - **C4 (no parallel drafting):** Closed — spec line 268 sequences M3 spec → Codex review → 3a spec; no parallel work.
   - Phase 4d amendment cross-check: `docs/specs/m2-phase4d-vault-migration-design.md:11–20` now carries the "Reviewer pivot" section, explicitly stating the Drive-path decision is unchanged but ACL provisioning moves to M3/M4. No contradiction remains between 4d and M3.

7. Readiness:      WARN — Phase 3a spec **can** be written confidently from this, with two caveats. (A) The Gemini-routing hedge at spec line 129 must resolve before 3b spec opens (not 3a, since 3a has no narrative generation). Evidence-based call below. (B) The custom-claims-writer architectural hole flagged in §2 must land in AD-1 before 3c spec opens.

---

GEMINI-ROUTING CALL (Moe's direct question)
-------------------------------------------

Spec line 129 hedges: *"routed via existing OpenClaw/AI Studio setup (verify the VM-side gateway can be called from a desktop-app client; likely the app calls Gemini directly using the existing GEMINI_API_KEY, with OpenClaw used only on server side)."*

**The OpenClaw gateway cannot serve the consultant's desktop app.** Evidence:
- `/home/moe_ikaros_ae/.openclaw/openclaw.json:583–587` — `"gateway": { "port": 18789, "mode": "local", "bind": "loopback" }`. The gateway binds to loopback on elara-vm. It is not publicly reachable.
- The gateway is reachable externally only via Tailscale Funnel on 443 (CLAUDE.md "Running Services" table), but auth is `"mode": "token"` + `"allowTailscale": true` (openclaw.json:600–603). A consultant's Mac would need to be on the IKAROS Tailnet with a valid device, which is true for Moe but **not** for external consultants when M4 multi-tenant lands.
- The gateway's `/v1/chat/completions` endpoint (openclaw.json:617–621) exists and works for server-to-server use (`packages/ikaros-command-center/src/app/api/donna/chat/route.ts:106` is a live example), but that's a Next.js server on elara-vm calling loopback — not a desktop app calling over the internet.
- `ikrs-workspace/src-tauri/src/` has no Gemini client today; only Claude CLI integration.

**The hedge's second half — "app calls Gemini directly using existing `GEMINI_API_KEY`" — is a security problem.** Per CLAUDE.md Golden Rule #4, `GEMINI_API_KEY` lives in `~/.openclaw/.env.gcp` and `~/.gemini/settings.json` on elara-vm (perms 600). Compiling it into the Tauri binary makes it **public** — the app ships to consultants' Macs and can be extracted from the binary with `strings`. That key is tied to `ikaros-portal`'s free AI Studio quota; once extracted, anyone can burn through the 250 RPD or rotate you into paid billing (replaying the Golden Rule #9 / "Vertex billing leak" incident pattern).

**Right call: neither "direct Gemini from desktop" nor "gateway on VM from desktop". Use a thin server-side proxy.** Specifically:
- Add a Cloud Function (or a route on `packages/ikaros-command-center` which already has Firebase Auth middleware per CLAUDE.md Command Center architecture) named `narrative-generate` that:
  1. Authenticates the caller via Firebase Auth ID token (same auth primitive AD-1 introduces for clients — reuse it for consultants).
  2. Enforces per-consultant rate limiting + Lester-owned budget guard.
  3. Calls Gemini Flash via the AI Studio REST API using a server-side-held `GEMINI_API_KEY` retrieved from Secret Manager.
  4. Returns the narrative to the desktop app.
- The desktop app holds **no** AI Studio key. Key rotation is a server-side operation with zero client distribution impact.
- Rename AD-2 from "Gemini Flash via OpenClaw" to "Gemini Flash via IKAROS narrative-generate Cloud Function" and update the risk at spec line 220 ("OpenClaw routing drift") to "narrative-generate model-prefix drift" — same discipline, correct locus.

This also enables the Haiku knob in AD-2 cleanly: `narrativeModel: 'flash' | 'haiku'` becomes a routing decision inside the Cloud Function, not a second client integration in the desktop app.

**If Moe wants "desktop app direct, no server":** acceptable only as a Phase-3b-local development shortcut (behind a dev-only build flag) with a mandatory migration to server-side before M4 ships. Document as tech debt with a closing date, not as the production path.

---

DECISION: APPROVED WITH CONDITIONS
Score: 9/10

Conditions (all must close before Phase 3a spec is written OR before the phase where they bind, as marked):
MC-1 (3a-blocking). Update AD-1 to name the custom-claims writer: a Cloud Function (or `ikaros-command-center` route) on `ikaros-portal`. Desktop app holds no Admin SDK credential.
MC-2 (3b-blocking). Replace AD-2's "via OpenClaw" routing with "via a server-side `narrative-generate` Cloud Function" per the Gemini-routing section above. Desktop app never holds `GEMINI_API_KEY`.
MC-3 (3a-blocking). AD-5 harden: "raw activity events are never written to Firestore." Remove the belt-and-suspenders parenthetical at spec line 205.
MC-4 (3b-blocking). Add per-engagement signal-gate at the bucketer: events with mismatched `engagementId` are dropped, not summarised. Prevents cross-engagement NDA bleed when the consultant multitasks.
MC-5 (3a-blocking). Promote open questions Q3 (timezone anchor) and Q7 (mid-engagement billing-period switch) into AD-6. Timezone: consultant-local wall-clock, timezone stored per submission.
MC-6 (3c-blocking). Declare audit-log immutability mechanism: Firestore rule denying updates on `auditLog`, or GCS-with-object-lock mirror. Pick one; land data model in 3a.
MC-7 (3c-blocking). Add consultant-vs-client dispute contract: client rejection is immutable; dispute is a new document, not an edit of the submission.
MC-8 (non-blocking, housekeeping). Remove "M3 closing Codex review" from Phase 3d scope line 159 — reviews are gates, not deliverables.

Decision for Moe
----------------
Spec is strong — 9/10 and genuinely closes all seven scope-lock conditions. **One security call is load-bearing before Phase 3b opens:** do not ship `GEMINI_API_KEY` inside the Tauri binary. Route narrative generation through a server-side Cloud Function / command-center route with Firebase-Auth-gated calls and a server-held key. This also future-proofs AD-2's Haiku knob and removes the OpenClaw-from-desktop ambiguity entirely. MC-1, MC-3, MC-4, MC-5 can land as spec amendments to `docs/specs/m3-timesheet-automation.md` today — they're not new architecture, they're tightening existing decisions. MC-6 and MC-7 can live in the phase specs they bind. MC-2 deserves a fresh architectural note (one or two paragraphs in AD-2, amended risk row) before Phase 3b spec is opened.

Phase 3a spec can be written now against MC-1/3/4/5 being addressed as amendments to this milestone spec.

Relevant files
--------------
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/specs/m3-timesheet-automation.md (AD-1 line 46, AD-2 line 56, AD-4 line 79, AD-5 line 88, line 129 Gemini hedge, line 205 Firestore-rule contradiction, line 234 open questions, line 253 conditions table)
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/specs/m2-phase4d-vault-migration-design.md (lines 11–20 "Reviewer pivot" amendment — cross-check passes)
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/docs/planning/2026-04-17-m3-scope-lock.md
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/.output/codex-reviews/2026-04-17-m3-scope-lock-review.md (prior conditions C1–C7)
- /home/moe_ikaros_ae/.openclaw/openclaw.json (lines 583–603 — gateway binds loopback, token+Tailscale auth)
- /home/moe_ikaros_ae/ikaros-platform/.architecture/OPENCLAW_INTEGRATION.md (Gateway architecture, Cloud Run limitations for Baileys, VM-only deployment rationale)
- /home/moe_ikaros_ae/ikaros-platform/packages/ikaros-command-center/src/app/api/donna/chat/route.ts:106 (live example of server-side `/v1/chat/completions` call — the pattern `narrative-generate` should copy, except replacing gateway with direct AI Studio REST)
- /home/moe_ikaros_ae/ikaros-platform/CLAUDE.md (Golden Rule #4 secrets posture, Golden Rule #9 Vertex billing leak precedent, Model Prefix Rules)
- /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/claude/types.rs:45 (AssistantMessage.usage — confirmed not emitted, AD-4 is genuinely new work)
