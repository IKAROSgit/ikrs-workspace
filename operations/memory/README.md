---
last_updated: 2026-05-06
updated_by: mac-claude (initial scaffold)
sources: []
---

# Operator Memory — Knowledge Graph

This directory is Moe's persistent knowledge graph. It is read by:
- **Heartbeat (Tier II)** — for engagement-specific context in prompts
- **WS Claude interactive** — at session start for continuity
- **WS Claude autonomous (Phase H)** — daily study session reads all,
  updates based on surface observations

## How to read

1. Start with `identity/moe.md` — who the operator is
2. Then `engagements/<active>.md` — current engagement context
3. Then relevant `people/`, `projects/`, `loops/` files
4. `patterns/` and `decisions/` for longer-term context
5. `recent/` for rolling 7-day summaries

## How to update

- **Append-only deltas.** Add new information; never delete existing
  content without archiving it first.
- **Atomic writes.** Write to a temp file, then rename into place.
  Never write directly to the target file (crash mid-write = data loss).
- **YAML frontmatter.** Every file starts with `---` frontmatter
  including `last_updated`, `updated_by`, and `sources`.
- **One concept per file.** Don't merge unrelated information.

## Audit expectation

Every write to this directory must be logged in `operations/audit/`.
The daily session (Phase H) writes a JSONL audit line for each
memory update with: timestamp, action, target path, byte count,
estimated tokens.
