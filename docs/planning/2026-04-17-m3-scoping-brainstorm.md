# M3 Scoping Brainstorm — 2026-04-17

**Status:** Open for decision
**Parent handoff:** `/home/moe_ikaros_ae/ikaros-platform/.output/2026-04-10-workspace-session-handoff.md`
**Prior work shipped:** M1 + what we *call* M2 Phases 1–4d (embedded Claude + skills + session UX + MCP + distribution polish). See `CHANGELOG.md` and `docs/specs/embedded-claude-architecture.md`.
**Governance:** Golden Rule #11 (Codex reviews) + superpowers `brainstorming` skill apply to this doc before any SPEC.md is written.

## Naming drift (flag honestly)

The original 2026-04-10 milestone plan (handoff §"What's Next" and parent M1 design) was:

| Original | Scope | Status |
|----------|-------|--------|
| M1 | Desktop shell, auth, MCP, Claude, tasks | SHIPPED |
| M2 | **Timesheet engine + manager dashboards** | **NOT STARTED** |
| M3 | Internal messaging + notifications | NOT STARTED |
| M4 | Polish, packaging, commercial release | SHIPPED (relabelled "M2 Phases 1–4d") |

**The honest read:** what code-history calls "M2" is the original M4. The original M2 — the commercial differentiator (AI-narrative timesheets against activity signals) — has been silently deferred. Picking the next milestone is also a choice about whether that differentiator still matters.

## Candidates

| ID | Candidate | One-paragraph scope & value | Depends on |
|----|-----------|------------------------------|------------|
| **X** | **Timesheet engine** | Passive activity capture (Claude session events, file writes, MCP calls, calendar blocks) → AI-generated daily/weekly timesheet narrative → consultant edits/approves → stored per-engagement. Value: the commercial differentiator. Consultants currently do timesheets manually or not at all; IKAROS bills clients against them. Feeds every downstream milestone (dashboards need data; manager approval needs a thing to approve). | None — all signals already exist in M2 (Claude stream, MCP events, file watcher, session registry). |
| **Y** | **Manager dashboards** | Line-manager visibility into consultant work: engagement list, hours logged, current activity, deliverables produced. Two delivery surfaces: (a) Drive ACLs (already partially free via ADR-013 vault-on-Drive) and (b) in-app dashboard view. Value: sales story to IKAROS PMs and to external consulting firms. | X (nothing to display without timesheet data); W for multi-user identity. |
| **Z** | **Internal messaging** | Slack-like channels, DMs, and agent chat inside the app. Value: reduces context-switching to Slack/WhatsApp. **Risk:** a messaging feature without multi-user is a toy; with multi-user it's a product. Also competes with existing Google Chat / OpenClaw (ADR-012) — we'd be rebuilding community-solved infra (Golden Rule #9 tension). | W (no users, no messages); arguably V (adopt OpenClaw instead of build). |
| **W** | **Multi-consultant isolation + line-manager ACL** | Turn the single-user app into a multi-tenant tool. Per-consultant Firestore isolation, IKAROS line-manager role with read access to their consultants' engagements, invitation flow, org-scoped settings. Enables commercial sale to other consulting firms. ADR-013 explicitly calls out M3 as when multi-user semantics must be resolved for GDrive write conflicts. | None strictly, but without X (timesheets) the commercial pitch is thin. |
| **V** | **Something else** considered-and-parked: (a) Telemetry/analytics backend (needed for X and Y anyway — fold in), (b) Billing/invoicing directly from timesheets (premature — M4), (c) Mobile companion (no). | — | — |

## Dependency graph

```
           (activity signals already exist from M2)
                        │
                        ▼
                   ┌────────┐
                   │   X    │  Timesheet engine
                   │ (data) │
                   └───┬────┘
                       │ produces the data that…
        ┌──────────────┼──────────────┐
        ▼              ▼              ▼
   ┌────────┐     ┌────────┐     ┌────────┐
   │   Y    │     │   W    │     │ billing│ (later M)
   │ dash   │     │ multi- │
   │ boards │     │ user   │
   └───┬────┘     └───┬────┘
       │              │
       └──────┬───────┘
              ▼
         ┌────────┐
         │   Z    │  messaging — only useful AFTER multi-user
         └────────┘
```

Key observation: **X is on the critical path for every other candidate.** Y is useless without X's data. W without X removes the commercial story. Z without W is a feature looking for users.

## Recommended sequence

**Ship X first, as the real M3.** Restore the original M2 scope under its new number. Concretely: timesheet engine is four phases mirroring the M2 run — (3a) activity-signal collector wired to Claude stream + file watcher + MCP events, persisted per engagement per day; (3b) narrative generator (prompt Claude with the day's raw signals, produce a draft timesheet in the engagement's `finance/` skill folder); (3c) review/approve UI (TimesheetView.tsx, edit narrative, mark approved, lock); (3d) export + retention (CSV/PDF export, Drive sync, configurable retention policy). After 3d ships and Codex passes, **the following milestone (M4) should be W (multi-consultant + line-manager ACL)** because ADR-013 already assumes this is coming and because Y and Z are both blocked on it. Y folds into W's phase 4b as a view over existing data. Z is deferred past M4 — re-evaluate whether to build in-app or adopt OpenClaw/Google Chat (Golden Rule #9).

## Open questions for Moe

1. **Activity signal fidelity.** What counts as "work" for the timesheet? Options: (a) any Claude session turn in an engagement; (b) Claude turns + MCP tool calls + file edits; (c) all of the above plus calendar events tagged to the engagement. More signals = better narrative, more plumbing.
2. **Narrative granularity.** Per-day summary, per-session summary, or both? Billing codes / activity categories (`consulting`, `admin`, `travel`)?
3. **Edit model.** Is the AI-generated timesheet a draft the consultant always edits, or an auto-approve-unless-flagged default? Affects approval-flow UX significantly.
4. **Manager visibility mechanism.** Drive ACLs on the engagement vault (already ~free from ADR-013) vs. in-app dashboard (requires W first). Is "manager opens Obsidian on their Mac and reads the engagement" sufficient for M3, deferring Y entirely?
5. **Is the original M2 (timesheets) still the commercial differentiator?** Confirming this is still the wedge, vs. something we learned during the M1+M2 build that's more valuable (e.g., the curated Claude experience itself is the product).
6. **Multi-consultant target.** Are we selling to other IKAROS consultants (internal) or other consulting firms (external)? Affects W's scope — internal can ride on Firebase Auth / ikaros.ae domain; external needs real tenancy.
7. **ADR-013 Phase 4d ordering.** The canonical-vault migration must land before external consultants install. Is it a prereq for W, or can W ship on the deprecated `~/.ikrs-workspace/vaults/` path and migrate later?
8. **Billing integration.** Out of scope for M3 — confirm. Timesheet export for now, Stripe/invoicing in a later M.

## What happens next

If Moe picks **sequence X → W → (Y folded into W) → re-evaluate Z**: we produce `docs/specs/m3-timesheet-engine-design.md` for X.phase-3a and `docs/superpowers/plans/2026-04-17-m3-phase3a-activity-collector.md`. Codex reviews both. Execute. If Moe picks a different order (e.g., W first because multi-tenant sale is closer than timesheet billing), we produce the spec for that instead. Either way: **no code until the spec and plan are Codex-approved.**

## Brainstorm process note

Per Golden Rule #11 and the `superpowers:brainstorming` skill, this document is intentionally a menu + recommendation, not a spec. The intent is that Moe reviews it (and optionally Codex does too) before anyone writes `SPEC.md`. Writing a spec for the wrong milestone is more expensive than an afternoon's brainstorm. Explicit flag: I recommended X partly because I noticed the original M2 was silently dropped — that's exactly the kind of drift this discipline exists to catch.
