# Session Handoff — M2 Brainstorm: Embedded Claude Architecture

**Date:** 2026-04-11
**Session:** Brainstorming + Codex review + condition fixes
**Status:** Spec APPROVED (all Codex conditions resolved)

## What Was Done

1. **Brainstormed** the embedded Claude Code architecture with CEO
   - 5 clarifying questions asked and answered
   - Decisions: curated assistant, OAuth auth, one-root-folder, living skills, Codex quality gates baked in

2. **Wrote full design spec** at `docs/specs/embedded-claude-architecture.md`
   - 16 sections covering: subprocess protocol, sandbox model, auth, permissions, orchestrator CLAUDE.md, 8 skill domains, skill sync, session management, crash recovery, offline behavior, security, cost model, risk register

3. **Codex reviewed** the spec → WARN 6.5/10 with 5 conditions
   - Review at `.output/codex-reviews/2026-04-11-embedded-claude-spec-review.md`

4. **Fixed all 5 conditions:**
   - C1: Stream-JSON schema rewritten from real CLI v2.1.92 output capture
   - C2: Risk register added (10 risks with severity, likelihood, mitigations)
   - C3: Permission handling resolved as design decision (Section 3.5)
   - C4: Hook event filtering strategy documented (Section 3.2.1)
   - C5: Process crash recovery protocol added (Section 3.14)

5. **Fixed all bonus items:**
   - "Specter" reference removed from legal CLAUDE.md
   - 8th quality gate added: Confidentiality
   - 8th skill folder added: talent/entertainment
   - Bash tool restricted via `--disallowed-tools`
   - Section numbering fixed (3.1 through 3.16)
   - Open questions resolved into spec sections

## What Changed

| File | Change |
|------|--------|
| `docs/specs/embedded-claude-architecture.md` | Full spec (new file, then 15+ edits for Codex fixes) |
| `.output/codex-reviews/2026-04-11-embedded-claude-spec-review.md` | Codex review (new file) |
| `.output/2026-04-11-m2-brainstorm-handoff.md` | This handoff (new file) |

## What's Next

1. **Implementation planning** — use the spec to create an M2 task plan (phases 1-4)
2. **Phase 1 first** — Core subprocess (claude.rs rewrite, stream parser, ChatView.tsx)
3. **Permission mode testing** — must verify `--permission-mode default` behavior in `--print` mode before committing to UI approval flow
4. **Elara briefing** — COO should be onboarded to manage this asset

## Key Design Decisions

- Claude Code CLI v2.1.92 as headless subprocess
- `--print --input-format stream-json --output-format stream-json --verbose --disallowed-tools Bash`
- OAuth via `claude auth login` (subscription-based, no API keys)
- 8 skill folders: communications, planning, creative, operations, legal, finance, research, talent
- 8 quality gates in orchestrator CLAUDE.md (including Confidentiality)
- One root workspace folder, user-selected via native dialog
- Curated assistant experience (tool calls → status cards, not raw output)
- Process crash recovery via try_wait() monitoring + orphan cleanup
