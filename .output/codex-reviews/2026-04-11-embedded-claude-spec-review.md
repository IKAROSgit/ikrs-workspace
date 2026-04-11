# CODEX REVIEW

**Subject:** Embedded Claude Code Architecture Spec
**Type:** spec-review (Tier 3 -- Design Spec)
**Date:** 2026-04-11
**Reviewed by:** Codex (claude-opus-4-6)
**Document:** `docs/specs/embedded-claude-architecture.md`
**Codebase state:** Post-M1 (commit 46a605e), existing ClaudeView.tsx + claude.rs being replaced

---

## VERDICTS

| # | Criterion | Verdict | Summary |
|---|-----------|---------|---------|
| 1 | Structural validation | **WARN** | Subprocess approach is sound in principle but the spec's documented stream-json event schema does not match the actual CLI output format. The process lifecycle model also has a significant gap around stdin pipe behavior. |
| 2 | Architectural consistency | **PASS** | All pieces (Rust session manager, stream parser, Tauri events, Zustand store, React components) fit together coherently. Clean separation of concerns. |
| 3 | Security audit | **WARN** | Three-layer sandbox model is well-designed but Layer 2 (CLAUDE.md instructions) is advisory, not enforced. Claude CLI's `--add-dir` semantics and the `Bash` tool provide escape hatches the spec does not address. |
| 4 | Completeness | **WARN** | Three significant gaps: (a) no handling for hook events in stream output, (b) no permission-prompt protocol, (c) no graceful process crash recovery. |
| 5 | Risk register | **FAIL** | No risk register section at all. A design spec for embedding a subprocess that has full filesystem access inside a sandboxed app MUST have an explicit risk register with mitigations. |
| 6 | Spec/plan alignment | **PASS** | Fully addresses all 5 CEO requirements. Strong improvement over M1. |
| 7 | Implementation readiness | **WARN** | Detailed enough for Phases 1-2 but Phase 3-4 are thin. The stream parser translation table needs correction before implementation begins. |

---

## OVERALL DECISION: WARN (6.5/10)

The spec demonstrates strong design thinking and correctly solves all 5 CEO-stated requirements. The subprocess architecture is the right approach. However, the spec contains several factual inaccuracies about the Claude CLI stream-json protocol that would cause implementation failures, and it lacks a risk register entirely. These must be corrected before implementation begins.

**Conditions for PASS (all required):**
1. Fix the stream-json event schema to match actual CLI output (Critical)
2. Add a risk register section (Critical)
3. Address permission-prompt handling for headless mode (Important)
4. Document hook event filtering strategy (Important)
5. Add process crash recovery protocol (Important)

---

## DETAILED FINDINGS

### F1 (Critical): Stream-JSON Event Schema Does Not Match Reality

**Location:** Section 3.2, "Output (Claude stdout -> Rust)" and "Stream Parser Translation Table"

The spec documents these event types:
```json
{"type":"system","subtype":"init","session_id":"uuid","tools":[...]}
{"type":"assistant","message":{"content":[...]}}
{"type":"result","subtype":"success","result":"..."}
```

I tested the actual Claude CLI v2.1.92 output with `--output-format stream-json --verbose` and the real output includes event types the spec does not account for:

- `{"type":"system","subtype":"hook_started",...}` -- Hook lifecycle events
- `{"type":"system","subtype":"hook_response",...}` -- Hook completion with stdout/stderr

The spec's `system.init` event type needs verification -- the actual init sequence may differ. The stream parser must handle (or explicitly filter) every event type the CLI emits, not just the happy-path subset.

**Impact:** If the Rust stream parser encounters an unrecognized event type and panics or drops the message, the entire session will break. This is the single most likely cause of "it works in testing but fails in production."

**Recommendation:** Before any implementation:
1. Capture a complete session's stream-json output (including tool use) and document every event type
2. Use `--include-hook-events` and `--include-partial-messages` flags (discovered in CLI help) to get the full picture
3. Design the parser with an explicit `_ => { /* log and skip unknown events */ }` fallback
4. Consider whether to pass `--bare` to suppress hooks (but note: `--bare` disables CLAUDE.md auto-discovery, which this spec explicitly depends on)

### F2 (Critical): No Risk Register

The spec has no "Risk Register" section. Per IKAROS platform standards (CLAUDE.md Golden Rule #11, Codex review criterion 5), every spec must have explicit risks with severity, likelihood, and mitigations.

Risks that should be documented:

| Risk | Severity | Likelihood | Notes |
|------|----------|------------|-------|
| R1: Claude CLI breaking changes between versions | HIGH | Medium | The stream-json format is not a stable API. CLI updates could change event structure without notice. |
| R2: Claude CLI `Bash` tool escapes sandbox | HIGH | Medium | Even with cwd set to engagement folder, `Bash(cd / && ...)` can access the entire filesystem. CLAUDE.md instructions are advisory only. |
| R3: Process zombie/orphan on app crash | MEDIUM | Medium | If the Tauri app crashes without triggering the exit handler, Claude child processes remain running. |
| R4: OAuth token expiry during session | MEDIUM | Low | Long-running sessions may hit token refresh boundaries. |
| R5: Consultant data cross-contamination | HIGH | Low | If the session manager has a bug mapping session_id to engagement, data could leak between engagements. |
| R6: CLI version incompatibility | HIGH | Medium | Spec written for v2.1.92. Future versions may remove `--print`, change `--input-format`, or alter auth flow. |
| R7: Disk space exhaustion | MEDIUM | Low | Claude Code session persistence in `~/.claude/` grows unbounded. Not within the workspace sandbox. |
| R8: `--bare` mode needed on consultant machines with hooks | MEDIUM | Medium | If consultants have their own Claude Code hooks configured globally, those hooks will fire inside the app context, causing unexpected behavior. |

### F3 (Important): Permission Prompt Handling Is Identified But Unsolved

**Location:** Section 5, Open Question #1

The spec correctly identifies that Claude CLI may prompt for tool permissions but leaves it as an open question. This is not acceptable for a design spec -- it is a core architectural decision.

From the CLI help output, I found the `--permission-mode` flag with these choices: `acceptEdits`, `auto`, `bypassPermissions`, `default`, `dontAsk`, `plan`.

**Recommendation:** The spec should mandate `--permission-mode plan` or `--permission-mode default` and implement a permission-request protocol:
1. When Claude CLI emits a permission request event, the stream parser translates it to a `claude:permission-request` Tauri event
2. The React UI shows a permission dialog: "Claude wants to [write file X / run command Y]. Allow?"
3. The consultant's response is piped back via stdin
4. If `--permission-mode` modes do not support this via stream-json (possible), the spec must state that `--dangerously-skip-permissions` is NOT acceptable and propose an alternative

This cannot be left as an open question because without it, the subprocess will block on stdin waiting for permission input, and the entire session will appear frozen to the consultant.

### F4 (Important): Hook Events Will Flood the Stream Parser

Related to F1. The actual CLI output starts with multiple `hook_started` and `hook_response` events before any assistant content. On a system with configured hooks (like the IKAROS platform VM which has Codex governance hooks), each session start produces 6+ hook events with potentially large payloads (the superpowers hook alone produces ~5KB of JSON).

**Impact:** The stream parser translation table in section 3.2 has no entry for hook events. They will either be silently dropped (bad -- no logging), cause parse errors (worse), or flood the UI with noise (worst).

**Recommendation:** Add a `system.hook_*` row to the translation table: "Hidden entirely. Optionally logged to debug panel." Consider passing `--bare` mode or using a clean CLAUDE.md-only configuration via `--setting-sources project` to suppress global hooks on consultant machines.

### F5 (Important): No Process Crash Recovery

The spec describes `kill_session()` for intentional shutdown but does not address what happens when:
1. The Claude CLI process crashes (segfault, OOM kill)
2. The process exits with a non-zero code (auth failure, API down)
3. The stdout pipe breaks (broken pipe)
4. The Tauri app is force-quit while a session is active

**Recommendation:** Add a "Process Health" section:
- The `ClaudeSessionManager` should monitor child process status via `try_wait()` on a background tokio task
- When the process exits unexpectedly, emit `claude:session-crashed` with the exit code and last stderr lines
- The UI should show a "Session ended unexpectedly. Restart?" dialog
- On app startup, scan for orphaned Claude processes from previous sessions and kill them

### F6 (Important): Layer 2 Sandbox (CLAUDE.md) Is Advisory, Not Enforced

**Location:** Section 3.3, "Layer 2: Claude CLI Working Directory"

The spec states: "Claude discovers the CLAUDE.md in that folder, which further instructs it to stay within the engagement boundary."

This is a soft constraint. Claude Code's `Bash` tool can trivially escape the cwd:
```bash
cat /etc/passwd
ls ~/Documents/
```

The `Read` and `Write` tools in Claude Code also accept absolute paths and are not restricted to cwd.

**Impact:** A consultant asking "find my tax documents" could cause Claude to read files outside the workspace. This is a data safety issue, not just a sandbox pedantry issue.

**Recommendation:**
1. Add `--allowedTools "Write Edit Read Glob Grep WebSearch WebFetch"` to the spawn args (explicitly excluding or restricting `Bash`)
2. Or use `--disallowedTools "Bash"` if Bash is deemed too risky for the consultant context
3. Document that Layer 2 is defense-in-depth, not a security boundary
4. Consider whether `--add-dir` should be used to explicitly scope filesystem access (though this flag expands access, not restricts it)

### F7 (Suggestion): `--print` Flag Semantics

**Location:** Section 3.2, Spawning

The spec uses `--print` for non-interactive mode. The CLI help states: "The workspace trust dialog is skipped when Claude is run with the -p mode. Only use this flag in directories you trust."

This is exactly what we want for the embedded case (trusted workspace folder), but the implication should be documented: `--print` mode skips the trust dialog. Since the app controls which directory is used, this is acceptable, but it should be explicitly noted.

### F8 (Suggestion): Concurrent Session Architecture Is Overly Conservative

**Location:** Section 3.8, "Engagement Switching" and Open Question #4

The spec enforces one session at a time, killing the previous when switching. While this is simpler, it conflicts with the `ClaudeSessionManager` being a `HashMap` (which supports multiple entries).

**Recommendation:** Keep the one-at-a-time behavior for M2, but:
1. Make `ClaudeSessionManager` enforce this as a policy (max_sessions = 1), not just a UI behavior
2. Document that the HashMap structure is intentionally over-engineered for future multi-session support
3. Add a session TTL so forgotten background sessions cannot persist indefinitely

---

## SKILL FOLDER REVIEW

### The 7 Categories

| Category | Verdict | Notes |
|----------|---------|-------|
| communications | **PASS** | Essential. Well-scoped. |
| planning | **PASS** | Core events management. 20% buffer rule is a great domain-specific quality gate. |
| creative | **PASS** | Good for an events company. |
| operations | **PASS** | Critical for day-of execution. |
| legal | **PASS** | Correct with disclaimer. DTCM-specific is smart for Dubai. |
| finance | **PASS** | VAT at 5%, AED primary currency -- domain-appropriate. |
| research | **PASS** | Venue scouting, vendor discovery -- high-value for events. |

**Overall: The 7 categories are well-chosen for events management.** No obvious gaps.

**One suggestion:** Consider whether a `talent/` or `entertainment/` category should be an 8th default. Events management in Dubai heavily involves talent booking (performers, speakers, hosts). The spec does mention custom folders can be added, but if talent coordination is a frequent need, it deserves a bundled template.

### Skill Template Quality

The individual CLAUDE.md templates are well-structured with:
- Clear capabilities list
- Domain-specific standards (good)
- File organization rules (good)
- ISO dates, AED currency, VAT -- all correct for the UAE context

**One issue with legal/CLAUDE.md:** The disclaimer "This is an organizational summary, not legal advice" is correct, but the template also says "Specter" (the IKAROS AI legal counsel agent). The IKRS Workspace is a standalone app for consultants -- it should not reference internal IKAROS agent names. Replace "flag for Specter/lawyer" with "flag for legal counsel review."

---

## CODEX QUALITY GATES REVIEW

**Location:** Section 3.5, "Quality Gates -- MANDATORY"

The 7 gates are:
1. Accuracy
2. Completeness
3. Brand Voice
4. Scope
5. Assumptions
6. File Hygiene
7. Self-Review

**Verdict: PASS with one enhancement.**

These are strong. The "Assumptions" gate (#5) is particularly good -- forcing Claude to declare assumptions prevents the #1 failure mode of AI assistants (confidently wrong). The "File Hygiene" gate (#6) ensures the folder structure stays organized.

**Enhancement:** Add an 8th gate: **Confidentiality** -- "Never include data from one client's engagement in another client's deliverables. Never reference internal IKAROS information in client-facing documents." This is important because the orchestrator CLAUDE.md is shared infrastructure, and consultants may switch between engagements frequently.

---

## OPEN QUESTIONS -- CODEX OPINIONS

### Q1: Permission prompts in headless mode

**Answer:** This is not optional -- it must be solved in the spec, not deferred. See Finding F3 above. The `--permission-mode` flag is the mechanism. Test `default` mode with stream-json to determine if permission requests appear as stream events. If they do, build a UI approval flow. If they do not, evaluate `--permission-mode plan` (which lets Claude plan but requires approval before execution).

### Q2: Session persistence location (~/.claude/)

**Answer:** Acceptable. `~/.claude/` is Claude Code's own data store, managed by the CLI, analogous to `~/.config/` or `~/Library/Application Support/`. It is not engagement data. The app should not attempt to move or manage this location. However, document this in the spec as a known data location outside the workspace sandbox, and note that session history in `~/.claude/` may contain engagement data (conversation transcripts). For consultants handling sensitive client data, this should be disclosed.

### Q3: MCP servers per-engagement

**Answer:** Yes, absolutely. This is one of the highest-value features. Wiring Gmail/Calendar/Drive MCP servers per-engagement means Claude can draft emails, check calendar availability, and pull Drive documents -- all within the engagement context. The `--mcp-config` flag is the right mechanism. The per-engagement config file should:
1. Live at `{engagement_path}/.mcp-config.json`
2. Be generated by `scaffold_engagement()` using the engagement's linked Google account credentials
3. Reference the same MCP server packages already in the M1 Cargo.toml/plan (gmail, calendar, drive, obsidian)
4. Be updated when the consultant re-authenticates

This should be added to Phase 3, not deferred.

### Q4: Concurrent sessions

**Answer:** One at a time is correct for M2. Multi-session adds complexity (memory, CPU, token cost tracking) with minimal user benefit. A consultant works on one engagement at a time. The HashMap in ClaudeSessionManager is fine as forward-compatible infrastructure.

### Q5: Offline behavior

**Answer:** Show a clear offline state:
1. On session spawn attempt: "Claude requires internet access. Check your connection."
2. During active session (connection drops mid-turn): "Connection interrupted. Your work is saved locally. Claude will resume when connected."
3. For the workspace itself (files, viewing): The app should work fully offline for file browsing, task management, and notes. Only Claude chat requires connectivity.

This should be a small subsection in the spec, not left as an open question.

---

## ALIGNMENT WITH CEO REQUIREMENTS

| CEO Requirement | Spec Coverage | Verdict |
|----------------|---------------|---------|
| 1. Claude functions WITHIN the app | Headless subprocess piped through Rust backend, rendered in ChatView.tsx | FULLY MET |
| 2. App is sandboxed to a physical folder | Three-layer sandbox (Tauri FS scope + cwd + macOS App Sandbox) | MET (with F6 advisory caveat) |
| 3. Pre-orchestrator CLAUDE.md with category subfolders | 7 skill domains, hierarchical CLAUDE.md, quality gates | FULLY MET |
| 4. Claude OAuth (no API keys) | Delegated to CLI auth, app never touches tokens | FULLY MET |
| 5. Curated assistant experience | Tool status cards, friendly labels, no raw output | FULLY MET |

**The spec nails all 5 requirements.** The design decisions are sound and well-reasoned.

---

## CODEBASE DELTA

Files that will be replaced/removed by this spec:

| File | Current State | Proposed State |
|------|--------------|----------------|
| `src-tauri/src/commands/claude.rs` | 89 lines, 3 commands (preflight, scaffold, launch) | Complete rewrite: 6 commands (auth_status, auth_login, spawn_session, send_message, kill_session, scaffold_engagement, check_skill_updates, sync_skills) |
| `src/views/ClaudeView.tsx` | 73 lines, "Open Claude Code" button | Replaced by ChatView.tsx (full chat interface) |
| `src/hooks/useClaude.ts` | 66 lines, external process tracking | Replaced by claudeStore.ts + useClaudeStream.ts |
| `src/types/index.ts` | `ClaudeSession` type (4 fields) | Needs new types: `ChatMessage`, `ToolActivity`, `ClaudeState` |
| `src/lib/tauri-commands.ts` | 3 Claude commands | Needs update: 8 new commands |
| `src/Router.tsx` | References ClaudeView | Must update lazy import to ChatView |

**No existing code will be silently broken.** The replacement is clean.

New Rust state that must be registered in `lib.rs`:
- `ClaudeSessionManager` (via `.manage()`)
- `SkillManager` (if implemented as managed state)

---

## POSITIVE OBSERVATIONS

1. **Excellent decision to not use `--bare`** -- The spec correctly wants CLAUDE.md auto-discovery, which `--bare` disables. This shows the author understands the CLI deeply.

2. **Friendly labels for tools** -- The `friendly_label()` function is exactly right. Showing "Writing venue-followup.md" instead of raw JSON is what makes this a curated experience.

3. **The "What This Replaces" table (section 3.13)** -- Clear migration mapping from old to new. This prevents the M1 code from lingering as dead weight.

4. **Skill sync with customization detection** -- The `.skill-version` mechanism with `customized_folders` tracking is a thoughtful approach to updating templates without stomping on consultant modifications.

5. **Phase sequencing** -- Phase 1 (core subprocess) before Phase 2 (skill system) is correct. Get the pipe working before adding orchestration.

6. **The 8 quality gates** (updated from 7) -- These are not generic. They are domain-specific for events management (brand voice, scope, assumptions, confidentiality). This is what makes it a professional tool, not a generic chat wrapper.

---

## DOCUMENTATION CHECKLIST (all resolved 2026-04-11)

- [x] Stream-json event schema corrected to match actual CLI output (real v2.1.92 capture, 8 event types documented)
- [x] Risk register section added to spec (Section 5, 10 risks with mitigations)
- [x] Permission-prompt handling resolved (Section 3.5, D7 — default mode with UI approval flow)
- [x] Hook event filtering documented (Section 3.2.1 — filter at parser, not CLI level)
- [x] Process crash recovery protocol added (Section 3.14 — try_wait monitoring, orphan cleanup, broken pipe handling)
- [x] Q3 (MCP servers) promoted from open question to Phase 3 requirement
- [x] Q5 (offline) promoted from open question to spec subsection (Section 3.15)
- [x] Legal CLAUDE.md: "Specter" reference removed → "legal counsel review"
- [x] 8th quality gate added: Confidentiality (gate #8 in orchestrator CLAUDE.md)
- [x] 8th default skill folder added: talent/entertainment (with CLAUDE.md template)
- [x] Bash tool restricted via `--disallowed-tools` (D8)
- [x] Section numbering fixed (3.1 through 3.16)
- [x] Open questions resolved into spec sections (Section 6: Resolved Questions)

---

## VERIFICATION PERFORMED

| Check | Method | Result |
|-------|--------|--------|
| Claude CLI version | `claude --version` | v2.1.92 -- matches spec |
| `--input-format stream-json` exists | `claude --help` | Confirmed |
| `--output-format stream-json` exists | `claude --help` | Confirmed |
| `--permission-mode` flag exists | `claude --help` | Confirmed (5 modes) |
| `--include-hook-events` flag exists | `claude --help` | Confirmed |
| `--include-partial-messages` flag exists | `claude --help` | Confirmed |
| `claude auth status --json` works | Executed | Returns `{loggedIn, authMethod, apiProvider}` |
| Actual stream-json output format | Tested with piped input | Produces `hook_started`, `hook_response` events not in spec |
| Existing codebase state | Read all 6 files being replaced | Clean M1 code, no conflicts |
| M1 Codex review | Read previous review | 7/10, conditions met |
