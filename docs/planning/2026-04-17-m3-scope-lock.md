# M3 Scope Lock — 2026-04-17 (Moe's Decisions)

**Status:** Locked pending Codex review of this document.
**Supersedes:** `2026-04-17-m3-scoping-brainstorm.md` (80 lines — the options-menu brainstorm).
**Codex input:** `.output/codex-reviews/2026-04-17-m3-scoping-review.md` (7/10 APPROVED WITH CONDITIONS — conditions addressed below by Moe's answers).

---

## Milestone Sequence (Locked)

**M3 = Timesheet automation** — reframed per Moe 2026-04-17: *"Wasn't even something I thought about beyond automating an administrative task that requires daily redundant and inefficient manual tracking and administration."*

This reframing matters. M3 is **not** positioned as a commercial differentiator — it's elimination of a tedious admin overhead that consultants currently do manually. The product bar is "makes the admin chore go away" not "best-in-market AI narrative timesheets." Lowers the pressure on narrative quality; raises the pressure on reliability + honesty + zero-interruption capture.

**Revised ordering** (unchanged from brainstorm recommendation):
1. **M3** — Timesheet automation (phases 3a signals → 3b narrative → 3c review UI → 3d export). Four sub-phases mirroring M2 cadence.
2. **M4** — Multi-consultant + white-label templating + client-facing review portal. Dashboards fold in as views over this data.
3. **Deferred past M4** — Internal messaging. Re-evaluate Z against OpenClaw/Google Chat adoption per Golden Rule #9. *Caveat:* OpenClaw lives on elara-vm, not inside Tauri, so "use OpenClaw" is a bridge-build, not a turnkey swap (Codex finding).

---

## Design Parameters (Moe's Answers)

### Activity capture

| Question | Answer |
|----------|--------|
| Signal fidelity | **Claude CLI token usage every 15 minutes.** Discrete, observable, low privacy impact (token counts ≠ content). |
| Narrative granularity | **Hourly entries.** A day becomes ~8 entries. Matches standard consulting-billing grain. |
| Additional signals (beyond token usage) | TBD by Codex review — see Open Design Questions below. Candidates: MCP tool calls (email sent, calendar event created, Drive file opened), file watcher on vault (when it lands in 3a), session start/stop events. |
| Capture interval (wall-clock) | Polls every 15 min while a session is active — no continuous stream storage. |

### Privacy posture (answers P1)

**Strict opt-in for everyone, including Moe.** No passive-by-default. Implies:
- First launch must present a consent flow explaining what will be captured and where it goes.
- Each engagement has its own capture-enabled toggle, default OFF.
- The consultant can pause/resume capture at any time; paused = zero signal emission.
- Captured data display must be clearly readable to the consultant (full transparency into what was recorded).

### Approval model

**Bypass per-entry permission — submit as a consolidated monthly timesheet.**

Day-to-day, capture runs without interrupting the consultant. At the end of each month:
- App compiles the month's captured activity into a client-facing timesheet.
- Consultant reviews, edits, submits.
- **Client** (not internal IKAROS manager) reviews and approves.
- No per-entry "does this look right?" interruptions during the work week.

### Reviewer model (this is a pivot)

Codex brainstorm assumed "line manager" was internal. **Moe clarified: reviewer is the client, not an IKAROS PM.** The client user:

- Can view the consultant's workstation state (what engagement is active, current session status)
- Can view docs / files / folders for their engagement (i.e. the engagement vault contents, read-only)
- Can view the running timesheet record
- Can approve or reject the consolidated monthly timesheet when submitted

**Implication:** M3 / M4 needs a **client-facing surface** — either as an external web portal, a read-only invited-user view inside the app, or Drive ACLs granting access to the vault. ADR-013 (canonical vault paths to Shared Drive) partially anticipated this; extension needed.

### Commercial target

**White-label, templated, adaptable.** Each consultant's installation is brandable for their client relationship. Client-facing portal/view carries the client's brand visually; consultant side remains IKAROS-branded.

Implications for architecture:
- Theme/brand config is per-engagement, not per-install.
- Product copy that mentions IKAROS must be sourced from a config, not hard-coded, on the client-facing surface.
- Firestore schema gains `engagement.brandConfig` or similar.

---

## Codex 2026-04-17 Conditions — Status

Codex gave the brainstorm 7/10 APPROVED WITH CONDITIONS. Tracking each against Moe's answers:

| Codex finding | Status |
|---------------|--------|
| **C1.** "All signals already exist" overstated — file watcher + MCP event bus + Calendar are new work | Acknowledged. Phase 3a scope expands to include building these, not just harvesting them. |
| **C2.** No privacy/consent treatment | **Closed by strict opt-in decision.** M3 Phase 3a includes a consent-flow sub-task. |
| **C3.** Storage locus unaddressed | **Partially open.** Token counts + narratives are Firestore (cross-device), activity log raw events are SQLite (local, bounded), vault markdown remains vault markdown per ADR-013. To be nailed in spec. |
| **C4.** OpenClaw-for-messaging glossed | **Acknowledged; messaging is deferred past M4** and will require a separate spec cycle at that time. |
| **C5.** AI narrative hallucination — legal-doc problem | **Partially closed by monthly-review model.** Consultant reviews + edits before client sees. But we must design the narrative system to be conservative — explicit "uncertain" markers, citation back to source signals, never inventing activity. |
| **C6.** UAE PDPL / client-NDA exposure | **New risk to register in spec.** Captured content (MCP tool args, file names, email subject lines) may contain client-confidential data. Storage must be per-engagement-isolated, and export must be redactable. |

Six of six addressed. The remaining work is writing the M3.3a spec with these decisions baked in, subject to Codex sign-off.

---

## Open Design Questions (Spec-Phase, Not Strategy)

These don't block scope lock; they're what the spec-writing phase solves next.

1. **Consent granularity.** Is consent per-engagement (toggle on creating each engagement) or global-with-per-engagement-override? Recommend global-off with explicit per-engagement opt-in for simplicity.
2. **What specific Claude CLI field do we poll for token usage?** Need to verify the stream surfaces total-tokens-per-turn or if we need to compute from prompt+completion counts. Phase 3a research task.
3. **Hourly bucketing edge cases.** A 3:45-4:15 session that straddles two hours — does it produce one entry (majority-owner hour) or two? Recommend majority-owner with a visible note.
4. **Narrative generation model.** Gemini Flash (free, fast, already integrated via OpenClaw) vs. Claude Haiku (fidelity, cost). For M3.3b. Recommend Flash first, Haiku as quality-upgrade knob.
5. **Client portal surface.** In-app invited-user view (requires per-client Firebase Auth) vs. external web portal (new deployment target) vs. Drive-native (no custom UI, ACLs on vault). Each trades development cost against client UX. For M4.
6. **White-label scope.** Just colors + logo, or also product name + feature toggles + custom Terms-of-Service? For M4.
7. **Monthly cycle mechanics.** Calendar month vs. 4-week rolling vs. consultant-defined billing period. For M3.3d.
8. **Export formats.** PDF invoice-style vs. CSV for client's accounting software vs. both. For M3.3d.

---

## What Happens Next

1. **Codex reviews this scope lock** (dispatching immediately after this document is committed).
2. If Codex PASS → **I draft M3 spec + Phase 3a spec in parallel.** Phase 3a scope: consent flow, activity-signal pipeline (file watcher + MCP event bus + token-usage poller + session registry surface), storage schema (SQLite local + Firestore shape), hourly bucketer.
3. Codex reviews M3 spec + Phase 3a spec.
4. If Codex PASS → plan-writing, then implementation.

Moe's ongoing actions (orthogonal to M3):
- Clone the now-public repo onto Mac, run `./tools/scripts/local-ad-hoc-sign.sh install` to start using the app daily.
- Upload `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to GitHub Secrets (before first `v*` tag).
- Apple Developer enrolment — arrives on its own timeline.
- Phase 4d vault migration when at Mac with Drive signed in.

None of those block M3 spec work.
