---
last_updated: 2026-05-06
updated_by: mac-claude (H.1 seeding)
sources: [docs/specs/m3-phase-h-right-hand.md]
---

# Decision: launchd plist for daily session (not scheduled-tasks MCP)

- **Date:** 2026-05-06
- **Context:** Phase H needs a daily scheduled Claude session. The spec
  originally proposed a scheduled-tasks MCP server. The adversarial
  challenge found this MCP doesn't exist.
- **Decision:** Use macOS launchd plist + wrapper shell script. Cap-hit
  resume reads checkpoint on next daily boot — no intra-day rescheduling.
- **Rationale:** launchd is native, reliable, and already understood by
  the operator. No new infrastructure to build. The wrapper script
  pattern keeps the plist stable while invocation logic evolves.
