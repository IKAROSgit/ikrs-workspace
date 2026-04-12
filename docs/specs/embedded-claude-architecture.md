# IKRS Workspace — Embedded Claude Code Architecture Spec

> **Status:** APPROVED — Codex WARN 6.5/10 → conditions fixed → PASS
> **Codex Review:** `.output/codex-reviews/2026-04-11-embedded-claude-spec-review.md`
> **Author:** Claude Code (brainstorming session with CEO)
> **Date:** 2026-04-11
> **Replaces:** M1 ClaudeView (external terminal launch)

## 1. Problem Statement

The M1 implementation launched Claude Code in an external terminal window. This was rejected by the CEO for three reasons:

1. **Not embedded** — the app should contain the full Claude experience, not delegate to Terminal.app
2. **Not sandboxed** — hardcoded `~/.ikrs-workspace/` path violates the principle that the app creates only a user-selected physical folder
3. **No orchestration** — no hierarchical CLAUDE.md structure to organize skill domains and enforce quality gates

## 2. Design Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | Claude Code CLI as headless subprocess piped through Rust backend | Proven CLI (v2.1.92), supports bidirectional stream-json, no need to reimplement Claude's tool system |
| D2 | Curated assistant experience | Consultant sees streaming text + tool status cards, not raw terminal output. Professional, not developer-facing. |
| D3 | One root workspace folder, auto-organized per engagement | User picks folder once via native dialog. Engagement subfolders auto-created. No hidden dotfiles outside this folder. |
| D4 | Claude OAuth (subscription-based) | No API keys. Consultant signs in with their Anthropic account. Claude CLI manages tokens in OS keychain. |
| D5 | Orchestrator CLAUDE.md per engagement with skill subfolders | 8 skill domains, each with its own CLAUDE.md. Root orchestrator enforces 8 quality gates including Confidentiality. |
| D6 | Living skills that sync with app updates | Skill templates are versioned, bundled with app, and mergeable on update. |
| D7 | Permission mode `default` with UI approval flow | Claude CLI permission prompts are surfaced as Tauri events; consultant approves/denies in the chat UI. |
| D8 | Bash tool restricted via `--disallowedTools` | Prevents Claude from escaping the engagement folder sandbox via shell commands with absolute paths. |

## 3. Architecture Overview

### 3.1 System Diagram

```
┌─────────────────────────────────────────────────────┐
│  IKAROS Workspace (Tauri 2.x)                       │
│                                                      │
│  ┌───────────────────────────────────────────────┐  │
│  │  React Webview                                 │  │
│  │                                                │  │
│  │  ChatView.tsx                                  │  │
│  │  ├── MessageList (streaming text bubbles)      │  │
│  │  ├── ToolActivityCard (status indicators)      │  │
│  │  ├── SessionIndicator (connected/thinking)     │  │
│  │  └── InputBar (send to Claude)                 │  │
│  │                                                │  │
│  │  claudeStore.ts (Zustand)                      │  │
│  │  useClaudeStream.ts (Tauri event listener)     │  │
│  └──────────────────┬────────────────────────────┘  │
│                     │ Tauri IPC                      │
│  ┌──────────────────┴────────────────────────────┐  │
│  │  Rust Backend                                  │  │
│  │                                                │  │
│  │  ClaudeSessionManager (Arc<Mutex<HashMap>>)    │  │
│  │  ├── spawn_session(engagement_id)              │  │
│  │  ├── send_message(session_id, text)            │  │
│  │  ├── kill_session(session_id)                  │  │
│  │  └── StreamParser (stdout → Tauri events)      │  │
│  │                                                │  │
│  │  SkillManager                                  │  │
│  │  ├── scaffold_engagement(root, client, ...)    │  │
│  │  ├── check_skill_updates(engagement_path)      │  │
│  │  └── sync_skills(engagement_path)              │  │
│  │                                                │  │
│  │  AuthManager                                   │  │
│  │  ├── check_auth_status()                       │  │
│  │  └── initiate_login()                          │  │
│  └───────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
         │                              │
         │ Claude CLI                   │ Filesystem
         │ (child process)              │ (sandboxed)
         ▼                              ▼
    Anthropic API               /Users/sara/IKAROS-Workspace/
    (subscription)              ├── .workspace-config.json
                                ├── .skill-templates/  (bundled, versioned)
                                ├── al-habtoor-gala/
                                │   ├── CLAUDE.md          (orchestrator)
                                │   ├── .skill-version     (template version marker)
                                │   ├── communications/
                                │   │   ├── CLAUDE.md
                                │   │   ├── meetings/
                                │   │   ├── drafts/
                                │   │   └── templates/
                                │   ├── planning/
                                │   │   ├── CLAUDE.md
                                │   │   ├── timelines/
                                │   │   ├── budgets/
                                │   │   └── vendors/
                                │   ├── creative/
                                │   │   ├── CLAUDE.md
                                │   │   ├── briefs/
                                │   │   └── content/
                                │   ├── operations/
                                │   │   ├── CLAUDE.md
                                │   │   ├── runsheets/
                                │   │   └── checklists/
                                │   ├── legal/
                                │   │   ├── CLAUDE.md
                                │   │   ├── contracts/
                                │   │   └── permits/
                                │   ├── finance/
                                │   │   ├── CLAUDE.md
                                │   │   ├── invoices/
                                │   │   └── expenses/
                                │   ├── research/
                                │   │   ├── CLAUDE.md
                                │   │   ├── venues/
                                │   │   └── market/
                                │   └── talent/
                                │       ├── CLAUDE.md
                                │       ├── shortlists/
                                │       └── riders/
                                └── dubai-expo-2026/
                                    └── (same structure)
```

### 3.2 CLI Subprocess Protocol

#### Spawning

```rust
let child = tokio::process::Command::new("claude")
    .args([
        "--print",
        "--input-format", "stream-json",
        "--output-format", "stream-json",
        "--verbose",
        "--disallowed-tools", "Bash",
    ])
    .current_dir(&engagement_path)  // CLAUDE.md auto-discovered here
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;
```

**Why these flags:**
- `--print` — non-interactive mode (no TUI). Note: skips workspace trust dialog, which is acceptable since the app controls the directory.
- `--input-format stream-json` — accept JSON messages on stdin (keeps process alive for bidirectional streaming)
- `--output-format stream-json` — structured JSON output on stdout
- `--verbose` — required for stream-json to emit all event types
- `--disallowed-tools "Bash"` — prevents Claude from escaping the engagement folder sandbox via shell commands with absolute paths (D8). The `Read`, `Write`, `Edit`, `Glob`, `Grep` tools respect working directory context. `WebSearch` and `WebFetch` are safe (no filesystem access).

**Why NOT these flags:**
- NOT `--bare` — we want CLAUDE.md auto-discovery, OAuth/keychain reads, and auto-memory. `--bare` disables all of these.
- NOT `--dangerously-skip-permissions` — we want Claude's permission system active so the consultant approves file changes (see Section 3.6 Permission Handling)
- NOT `--system-prompt` — the orchestrator CLAUDE.md in the engagement folder provides all context via auto-discovery
- NOT `--no-session-persistence` — we want session resume capability (`--resume` flag)
- NOT `--setting-sources project` — would suppress user-level settings; reserved as future option if hook noise becomes problematic

#### Input (Rust → Claude stdin)

```json
{"type":"user","content":[{"type":"text","text":"Draft a follow-up email to the venue about the March 15 walkthrough"}]}
```

#### Output (Claude stdout → Rust)

Messages arrive as newline-delimited JSON (NDJSON). The complete event taxonomy below was **captured from real Claude CLI v2.1.92 output** on 2026-04-11. Every event has a `type` field and a `session_id`.

```json
// 1. HOOK LIFECYCLE — multiple per session start, from user's global Claude hooks
//    Can produce 5KB+ payloads (e.g., superpowers hook injects full skill content)
{"type":"system","subtype":"hook_started","hook_id":"uuid","hook_name":"SessionStart:startup","hook_event":"SessionStart","session_id":"uuid"}
{"type":"system","subtype":"hook_response","hook_id":"uuid","hook_name":"SessionStart:startup","output":"...","stdout":"...","stderr":"","exit_code":0,"outcome":"success","session_id":"uuid"}

// 2. SESSION INIT — after all hooks complete. Contains available tools, model, mcp_servers.
{"type":"system","subtype":"init","cwd":"/path","session_id":"uuid","tools":["Read","Write","Edit","Glob","Grep","WebSearch","WebFetch"],"mcp_servers":[],"model":"claude-sonnet-4-6","permissionMode":"default","claude_code_version":"2.1.92"}

// 3. ASSISTANT THINKING — extended thinking block (if model supports it)
{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"Let me check the file...","signature":"base64..."}],"usage":{"input_tokens":3,"output_tokens":6}}}

// 4. ASSISTANT TOOL USE — Claude invokes a tool
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_014xxx","name":"Read","input":{"file_path":"/path/file.md"},"caller":{"type":"direct"}}]}}

// 5. TOOL RESULT — appears as type:"user" with tool_result content (NOT type:"assistant")
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"file contents...","is_error":false,"tool_use_id":"toolu_014xxx"}]},"tool_use_result":"file contents..."}

// 6. ASSISTANT TEXT — Claude's text response to the consultant
{"type":"assistant","message":{"content":[{"type":"text","text":"The file contains..."}]}}

// 7. RATE LIMIT EVENT — subscription/API rate limit status
{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1775905200,"rateLimitType":"five_hour"}}

// 8. TURN COMPLETE — session turn finished, includes cost and duration
{"type":"result","subtype":"success","is_error":false,"duration_ms":29987,"num_turns":2,"result":"final text","stop_reason":"end_turn","session_id":"uuid","total_cost_usd":0.10}
```

**Critical parser rules:**
1. The parser MUST handle unknown event types gracefully: `_ => { log::debug!("Unknown stream event type: {}", raw_type); }` — never panic.
2. Tool results come as `type:"user"` (not `type:"assistant"`) — the parser must handle this.
3. Content blocks within assistant messages can be: `text`, `thinking`, `tool_use` — all in the same message.
4. Hook events are filtered (see Section 3.2.1 Hook Filtering Strategy).
5. New CLI versions may add event types without notice — the parser must be forward-compatible.

#### 3.2.1 Hook Filtering Strategy (C4 fix)

Consultant machines may have their own global Claude Code hooks configured. These hooks fire on every session start and can produce large payloads (5KB+ per hook).

**Strategy:** Filter hooks at the parser level, not at the CLI level.

**Why not `--bare`:** Using `--bare` would suppress hooks but also disables CLAUDE.md auto-discovery, OAuth/keychain reads, and auto-memory — all of which we depend on. This is an unacceptable trade-off.

**Why not `--setting-sources project`:** This restricts settings to the project's `.claude/settings.json` only, which would suppress global hooks. However, it would also suppress user-level settings the consultant may need. Acceptable as a future option if hook noise becomes a real problem.

**Parser behavior:**
```rust
match (event_type.as_str(), event_subtype.as_deref()) {
    ("system", Some("hook_started")) => { /* silently drop — do not emit Tauri event */ },
    ("system", Some("hook_response")) => { /* silently drop — optionally log to debug panel */ },
    ("system", Some("init")) => { /* emit claude:session-ready */ },
    ("assistant", _) => { /* parse content blocks → emit appropriate events */ },
    ("user", _) => { /* tool result → emit claude:tool-end */ },
    ("rate_limit_event", _) => { /* silently drop — internal bookkeeping */ },
    ("result", _) => { /* emit claude:turn-complete or claude:error */ },
    _ => { log::debug!("Unknown stream event: {}", event_type); },
}
```

#### Stream Parser Translation Table

| Raw event type | Raw subtype/content | Tauri event | Payload | UI rendering |
|---------------|--------------------|----|---------|--------------|
| `system` | `hook_started` | *(filtered)* | — | Not shown |
| `system` | `hook_response` | *(filtered)* | — | Not shown (debug panel only) |
| `system` | `init` | `claude:session-ready` | `{session_id, tools, model, cwd}` | Green dot on session indicator |
| `assistant` | content: `thinking` | *(filtered)* | — | Not shown (internal reasoning) |
| `assistant` | content: `text` | `claude:text-delta` | `{text, message_id}` | Append text to current chat bubble |
| `assistant` | content: `tool_use` | `claude:tool-start` | `{tool_id, tool_name, friendly_label}` | Show tool activity card (spinner) |
| `user` | content: `tool_result` | `claude:tool-end` | `{tool_id, success, summary}` | Update card to checkmark/error |
| `rate_limit_event` | — | *(filtered)* | — | Not shown |
| `result` | `success` | `claude:turn-complete` | `{cost_usd, session_id, duration_ms}` | Re-enable input bar, show cost |
| `result` | `error` | `claude:error` | `{error_message}` | Error toast notification |
| stderr | *(line)* | `claude:stderr` | `{line}` | Debug panel only |
| *(unknown)* | — | *(logged, dropped)* | — | Not shown |

#### Friendly Labels for Tools

```rust
fn friendly_label(tool_name: &str, input: &serde_json::Value) -> String {
    match tool_name {
        "Write" => format!("Writing {}", short_path(input["file_path"].as_str())),
        "Edit" => format!("Editing {}", short_path(input["file_path"].as_str())),
        "Read" => format!("Reading {}", short_path(input["file_path"].as_str())),
        "Glob" => format!("Searching files matching {}", input["pattern"].as_str().unwrap_or("...")),
        "Grep" => format!("Searching for \"{}\"", input["pattern"].as_str().unwrap_or("...")),
        "WebSearch" => format!("Searching the web for \"{}\"", input["query"].as_str().unwrap_or("...")),
        "WebFetch" => format!("Fetching {}", input["url"].as_str().unwrap_or("...")),
        // Bash is disallowed via --disallowed-tools (D8) — no label needed
        _ => format!("Working..."),
    }
}
```

### 3.3 Sandbox Architecture

#### Three Isolation Layers

**Layer 1: Tauri Filesystem Scope**

```json
// capabilities/default.json — dynamic scope
{
  "permissions": [
    {
      "identifier": "fs:allow-read",
      "allow": [{ "path": "$WORKSPACE_ROOT/**" }]
    },
    {
      "identifier": "fs:allow-write",
      "allow": [{ "path": "$WORKSPACE_ROOT/**" }]
    }
  ]
}
```

The workspace root is set at first launch via `dialog:open` (native folder picker). The Tauri app physically cannot access files outside this folder through its own APIs.

**Layer 2: Claude CLI Working Directory**

Claude Code respects its `cwd` as the primary workspace. Each session is spawned with `cwd` = the engagement subfolder. Claude discovers the `CLAUDE.md` in that folder, which further instructs it to stay within the engagement boundary.

**Layer 3: macOS App Sandbox (distribution)**

When distributed via DMG/App Store:
- App Sandbox entitlement enabled
- File access limited to user-selected folders (Security-Scoped Bookmarks)
- Network access limited to Anthropic API endpoints
- No access to Contacts, Calendar, Photos, etc.

#### First Launch Flow

```
1. App opens for the first time
2. Welcome screen: "Choose where IKAROS Workspace stores your files"
3. Native macOS folder picker (tauri-plugin-dialog)
4. User selects: /Users/sara/IKAROS-Workspace/
5. App creates: .workspace-config.json (metadata)
6. Path stored in SQLite config DB
7. Tauri fs scope updated to this path
8. All engagement folders auto-created here going forward
```

### 3.4 Authentication

#### OAuth Flow

Claude Code CLI manages its own authentication. The app delegates to it:

```
1. App calls: claude auth status (check if logged in)
2. If not authenticated:
   a. Show "Sign in to Claude" button in app
   b. On click: spawn `claude auth login`
   c. Claude CLI opens system browser → Anthropic OAuth page
   d. User logs in with their Anthropic account (Max/Team/Enterprise)
   e. Token stored in OS keychain by Claude CLI
   f. App detects auth complete, enables chat
3. If authenticated:
   a. Show account info (email, plan tier)
   b. Enable chat immediately
```

**Commands used:**
- `claude auth status` — returns JSON: `{"loggedIn": true, "authMethod": "oauth_token", "apiProvider": "firstParty"}` (verified 2026-04-11)
- `claude auth login` — triggers OAuth flow in system browser
- `claude auth logout` — clears stored tokens

**What the app NEVER touches:**
- API keys
- OAuth tokens
- Keychain entries
- Billing information

All of this is managed by Claude CLI. The app just asks "are you logged in?" and "please log in."

### 3.5 Permission Handling (D7)

Claude CLI has a permission system that asks the user to approve tool actions (file writes, edits, etc.). In headless mode (`--print`), this must be handled explicitly.

#### Design Decision

Use `--permission-mode default` (the CLI default). In this mode, Claude Code emits permission request events when it wants to use a tool that requires approval. The Rust stream parser translates these into Tauri events, and the React UI presents an approval dialog to the consultant.

**Why not `--dangerously-skip-permissions`:** Auto-approving all tool actions removes the consultant's control. In a curated assistant experience, the consultant should approve file modifications — this is a trust-building feature, not a burden.

**Why not `--permission-mode plan`:** Plan mode prevents Claude from executing any tools and only produces plans. This removes the primary value of the embedded experience (doing work, not just planning).

#### Permission Flow

```
1. Claude wants to write a file
2. CLI emits permission request on stdout (stream-json event)
3. Rust parser detects permission request → emits claude:permission-request
4. React UI shows: "Claude wants to write communications/drafts/vendor-email.md — Allow?"
5. Consultant clicks Allow or Deny
6. React sends response via Tauri IPC → Rust writes approval/denial to stdin
7. Claude proceeds or adjusts
```

**Implementation note:** If testing reveals that `--permission-mode default` does NOT emit permission requests as stream-json events in `--print` mode (i.e., it blocks silently on stdin), the fallback is `--permission-mode acceptEdits` which auto-approves file edits but still logs them. This would degrade the experience from "consultant approves" to "consultant sees what happened" but avoids frozen sessions. This MUST be tested in Phase 1 before committing to either approach.

### 3.6 Orchestrator CLAUDE.md Template

This is the root `CLAUDE.md` placed in each engagement folder. Variables in `{braces}` are interpolated at scaffold time.

```markdown
# {client_name} — {engagement_title}

You are an IKAROS Workspace assistant helping {consultant_name} manage this engagement.
You work exclusively within this folder. Do not access files outside it.

## Context
- **Client:** {client_name}
- **Engagement:** {engagement_description}
- **Consultant:** {consultant_name} ({consultant_email})
- **Timezone:** {timezone}
- **Started:** {start_date}

## Skill Domains

When the consultant's request matches a domain, change to that subfolder and
read its CLAUDE.md for domain-specific instructions before proceeding.

| Folder | Domain | Use when |
|--------|--------|----------|
| `communications/` | Email, client comms, meeting notes | Drafting, responding, summarizing any communication |
| `planning/` | Timelines, logistics, budgets | Event planning, scheduling, resource allocation |
| `creative/` | Content, design briefs, presentations | Brand content, pitch decks, social media copy |
| `operations/` | Run sheets, SOPs, vendor coordination | Execution checklists, day-of operations |
| `legal/` | Contracts, permits, compliance | NDAs, DTCM permits, vendor agreements |
| `finance/` | Invoicing, expenses, P&L | Billing, cost tracking, financial reports |
| `research/` | Market research, venue scouting | Competitor analysis, vendor discovery, pricing |
| `talent/` | Talent booking, entertainment | Performers, speakers, hosts, technical riders |

## Quality Gates — MANDATORY

Before presenting ANY deliverable to the consultant, you MUST pass all 8 gates.
If any gate fails, state which one and why before proceeding.

1. **Accuracy** — Verify all facts, dates, and amounts against files in this workspace
2. **Completeness** — Does the output fully address what was asked?
3. **Brand Voice** — Professional, warm, IKAROS-standard (see `creative/CLAUDE.md` for tone guide)
4. **Scope** — Is this within the engagement boundaries defined above?
5. **Assumptions** — Explicitly flag every assumption you made
6. **File Hygiene** — Save deliverables in the correct skill subfolder with ISO-dated filenames
7. **Self-Review** — Re-read your complete output once before presenting it
8. **Confidentiality** — Never include data from one client's engagement in another client's deliverables. Never reference internal company information in client-facing documents.

## Working Conventions
- ISO dates in filenames: `YYYY-MM-DD-description.md`
- Drafts go in `{domain}/drafts/`, finals in `{domain}/final/`
- Meeting notes go in `communications/meetings/`
- All monetary amounts in AED with USD equivalent
- Always include next steps / action items in communications
```

### 3.7 Skill Domain Templates

Each skill folder contains a `CLAUDE.md` with domain-specific instructions. These are the 8 bundled templates:

#### communications/CLAUDE.md
```markdown
# Communications Skill

## Capabilities
- Client emails (follow-ups, proposals, confirmations, thank-yous)
- Internal memos and team updates
- Meeting notes with action items
- Vendor correspondence
- Stakeholder briefings

## Voice & Tone
- Professional but approachable — never stiff, never casual
- Every email includes clear next steps
- Use bullet points for action items
- Sign off as: {consultant_name}, IKAROS

## File Organization
- `meetings/` — Meeting notes (YYYY-MM-DD-subject.md)
- `drafts/` — Email drafts awaiting consultant review
- `final/` — Approved and sent communications
- `templates/` — Reusable email templates
```

#### planning/CLAUDE.md
```markdown
# Planning Skill

## Capabilities
- Event timelines with milestones and dependencies
- Budget planning, tracking, and variance analysis
- Venue logistics and floor plan notes
- Vendor coordination schedules
- Risk assessment and contingency plans
- Guest list management and RSVP tracking

## Standards
- All dates in ISO format (YYYY-MM-DD)
- Budgets in AED with USD equivalent in parentheses
- Timelines include minimum 20% buffer on every task
- Every plan MUST have a risk section
- Dependencies explicitly stated between tasks

## File Organization
- `timelines/` — Project timelines and gantt-style plans
- `budgets/` — Cost breakdowns and actuals tracking
- `vendors/` — Vendor briefs, quotes, and selection matrices
- `risk/` — Risk registers and contingency plans
- `guests/` — Guest lists and RSVP tracking
```

#### creative/CLAUDE.md
```markdown
# Creative Skill

## Capabilities
- Event concepts and mood boards (text descriptions)
- Design briefs for external designers
- Presentation decks (markdown outline → content)
- Social media copy and content calendars
- Brand guidelines enforcement
- Signage and collateral copy

## Standards
- Always reference client brand guidelines if available
- Presentations: max 10 words per slide title, 3 bullets per slide
- Social media: platform-specific character limits respected
- Design briefs include: objective, audience, deliverables, timeline, references

## File Organization
- `briefs/` — Design and creative briefs
- `content/` — Written content (copy, captions, scripts)
- `presentations/` — Slide deck outlines and content
- `brand/` — Client brand guidelines and assets references
```

#### operations/CLAUDE.md
```markdown
# Operations Skill

## Capabilities
- Event run sheets (minute-by-minute schedules)
- Standard Operating Procedures (SOPs)
- Vendor load-in/load-out schedules
- Staff assignment matrices
- Equipment and inventory checklists
- Post-event debrief templates

## Standards
- Run sheets in 15-minute increments minimum
- Every checklist item has an owner and deadline
- SOPs include: purpose, scope, steps, responsible party, escalation
- Equipment lists include quantities and backup sources

## File Organization
- `runsheets/` — Day-of event run sheets
- `checklists/` — Pre-event, day-of, and post-event checklists
- `sops/` — Standard operating procedures
- `staffing/` — Staff assignments and contact sheets
```

#### legal/CLAUDE.md
```markdown
# Legal Skill

## Capabilities
- Contract review summaries (NOT legal advice — flag for legal counsel review)
- DTCM permit requirement checklists (Dubai events)
- NDA and confidentiality agreement templates
- Vendor agreement term sheets
- Insurance requirement summaries
- Compliance checklists by jurisdiction

## Standards
- ALWAYS include disclaimer: "This is an organizational summary, not legal advice"
- Flag any clause that needs legal review with [LEGAL REVIEW REQUIRED]
- UAE-specific: reference DTCM regulations where applicable
- All amounts in AED
- Include expiry dates and renewal terms prominently

## File Organization
- `contracts/` — Contract summaries and term sheets
- `permits/` — Permit applications and requirement checklists
- `compliance/` — Regulatory compliance documentation
- `templates/` — NDA, vendor agreement, and other templates
```

#### finance/CLAUDE.md
```markdown
# Finance Skill

## Capabilities
- Invoice preparation and tracking
- Expense categorization and reporting
- Budget vs. actuals variance analysis
- Payment schedule management
- Profit & loss summaries per engagement
- Cost estimation for proposals

## Standards
- All amounts in AED (USD equivalent in parentheses)
- VAT (5%) calculated and shown separately
- Invoice numbers: {client_slug}-INV-YYYYMMDD-NNN
- Expense categories: venue, catering, AV, decor, talent, logistics, marketing, admin, contingency
- Payment terms stated on every invoice

## File Organization
- `invoices/` — Prepared invoices
- `expenses/` — Expense reports and receipts log
- `budgets/` — Budget tracking (linked from planning/)
- `reports/` — P&L summaries and financial reports
```

#### research/CLAUDE.md
```markdown
# Research Skill

## Capabilities
- Venue scouting and comparison matrices
- Vendor discovery and evaluation
- Market pricing research
- Competitor event analysis
- Industry trend summaries
- Client company background research

## Standards
- Always cite sources with URLs or document references
- Comparison matrices: minimum 5 criteria, weighted scoring
- Price ranges: show low/mid/high with source for each
- Date all research (information decays rapidly in events industry)
- Flag assumptions vs. verified facts

## File Organization
- `venues/` — Venue research and comparison documents
- `vendors/` — Vendor evaluations and shortlists
- `market/` — Market research and trend reports
- `competitors/` — Competitor analysis
```

#### talent/CLAUDE.md
```markdown
# Talent & Entertainment Skill

## Capabilities
- Talent sourcing and shortlisting (performers, speakers, hosts, DJs)
- Technical rider review and requirements tracking
- Talent fee negotiation preparation
- Performance schedule coordination
- Artist hospitality and logistics planning
- Entertainment package comparison

## Standards
- All fees in AED with USD equivalent
- Technical riders must be cross-referenced with venue capabilities
- Always include backup talent options
- Performance schedules include setup, soundcheck, performance, and teardown
- Dietary and hospitality requirements documented separately

## File Organization
- `shortlists/` — Talent shortlists with profiles and fees
- `riders/` — Technical and hospitality riders
- `schedules/` — Performance timelines
- `contracts/` — Talent agreement summaries (flag for legal counsel review)
```

### 3.8 Skill Sync & Evolution

#### Version Tracking

Each engagement folder contains `.skill-version`:
```json
{
  "template_version": "1.0.0",
  "scaffolded_at": "2026-04-11T10:00:00Z",
  "customized_folders": []
}
```

The app binary bundles skill templates at a known version. On engagement open:

```
1. Read .skill-version from engagement folder
2. Compare with bundled template version (semver comparison: bundled > installed)
3. If bundled > engagement:
   a. Check which folders have been customized (CLAUDE.md content differs from bundled default)
   b. For un-customized folders: show "update available" badge in SkillStatusPanel
   c. For customized folders: mark as "custom" (protected, never auto-updated)
   d. Consultant clicks "Update N skills" to apply updates to un-customized folders only
   e. Update .skill-version to bundled version after applying
4. If equal: no action
```

> **Design decision**: Updates require explicit user action rather than auto-applying silently.
> This is safer — the consultant sees exactly which domains will be updated before confirming.
> Customized domains are always protected regardless.

#### Custom Skills

Consultants can create additional skill folders (e.g., `hospitality/`, `entertainment/`).
Custom folders are tracked in `.skill-version.customized_folders` and are never overwritten by sync.

### 3.9 Session Management

#### Session Lifecycle

```
spawn_session(engagement_id)
  → creates ClaudeSession in manager
  → spawns claude CLI process
  → emits claude:session-ready when init received
  → returns session_id

send_message(session_id, text)
  → writes JSON to process stdin
  → stream parser emits events as they arrive
  → emits claude:turn-complete when result received

kill_session(session_id)
  → sends SIGTERM to process
  → cleans up from manager
  → emits claude:session-ended
```

#### Engagement Switching

When consultant switches to a different engagement:
1. Kill current session (if any)
2. Spawn new session with new engagement's folder as cwd
3. New session auto-discovers the new engagement's CLAUDE.md
4. Chat history resets (previous engagement's history is in Claude's session persistence)

#### Session Resume

Claude Code persists sessions by default. The app can resume:
1. Store `session_id` per engagement in SQLite
2. On re-opening engagement, spawn with `--resume {session_id}`
3. Claude continues from where the consultant left off

### 3.10 Rust Backend: New Commands

#### Commands to ADD

```rust
// Authentication
pub async fn claude_auth_status() -> Result<AuthStatus, String>
pub async fn claude_auth_login(app: AppHandle) -> Result<(), String>

// Session management
pub async fn spawn_claude_session(
    engagement_id: String,
    engagement_path: String,
    state: State<'_, ClaudeSessionManager>,
    app: AppHandle,
) -> Result<String, String>  // returns session_id

pub async fn send_claude_message(
    session_id: String,
    message: String,
    state: State<'_, ClaudeSessionManager>,
) -> Result<(), String>

pub async fn kill_claude_session(
    session_id: String,
    state: State<'_, ClaudeSessionManager>,
) -> Result<(), String>

// Skill management
pub async fn scaffold_engagement(
    workspace_root: String,
    client_slug: String,
    client_name: String,
    engagement_title: String,
    consultant_name: String,
    consultant_email: String,
    timezone: String,
    description: String,
) -> Result<String, String>  // returns engagement folder path

pub async fn check_skill_updates(
    engagement_path: String,
) -> Result<SkillUpdateStatus, String>

pub async fn sync_skills(
    engagement_path: String,
    folders_to_update: Vec<String>,
) -> Result<(), String>
```

#### Commands to REMOVE

```rust
// DELETE — replaced by new commands above
pub async fn claude_preflight() -> ...
pub async fn scaffold_claude_project() -> ...
pub async fn launch_claude() -> ...
```

### 3.11 React Frontend: New Components

#### ChatView.tsx (replaces ClaudeView.tsx)

Full chat interface:
- Scrollable message list with auto-scroll
- Streaming text rendered as it arrives (character by character)
- Tool activity cards between messages
- Input bar with send button (disabled while Claude is responding)
- Session status indicator (connected / thinking / error)

#### ToolActivityCard.tsx

Compact status card:
- Collapsed (default): icon + friendly label + spinner/checkmark
- Examples: "📝 Writing venue-followup.md..." → "✅ Written venue-followup.md"
- No raw JSON, no file contents, no command output

#### SessionIndicator.tsx

Top bar widget:
- 🟢 Connected (session active, idle)
- 🟡 Thinking (Claude is responding)
- 🔴 Disconnected (no session or process died)
- Click to see: engagement name, session duration, token cost

#### claudeStore.ts (Zustand)

```typescript
interface ClaudeState {
  sessionId: string | null;
  status: 'disconnected' | 'connected' | 'thinking' | 'error';
  messages: ChatMessage[];
  activeTools: ToolActivity[];
  totalCostUsd: number;
  error: string | null;

  // Actions
  addTextDelta: (messageId: string, text: string) => void;
  startTool: (toolId: string, label: string) => void;
  endTool: (toolId: string, success: boolean) => void;
  completeTurn: (costUsd: number) => void;
  setError: (error: string) => void;
  reset: () => void;
}
```

### 3.12 Security Considerations

| Concern | Mitigation |
|---------|-----------|
| Claude CLI filesystem access | `--disallowed-tools Bash` (D8); `cwd` scoped to engagement folder; CLAUDE.md instructs boundary; Read/Write absolute path escape is accepted residual risk (R2, R10) |
| API tokens | Managed by Claude CLI in OS keychain — app never touches them |
| Network access | Claude CLI only talks to Anthropic API; Tauri CSP restricts webview |
| Process escape / zombies | SIGTERM on session kill; orphan cleanup on startup; health monitoring via try_wait() (Section 3.14) |
| Cross-engagement data | One session at a time (max_sessions=1); Quality gate #8 (Confidentiality); separate cwd per engagement |
| Workspace root outside app | Security-Scoped Bookmarks (macOS) persist folder access permission |
| External data in `~/.claude/` | Accepted (R7). Claude CLI manages session persistence. May contain conversation transcripts. Documented. |
| Consultant's global hooks | Filtered at parser level (Section 3.2.1). Hooks execute but output is not shown. |

### 3.13 Cost Model

- **For the consultant:** Anthropic subscription (Max plan or Team plan) — Claude CLI uses subscription tokens
- **For IKAROS:** $0 per consultant per month for AI inference — the consultant's subscription covers it
- **App distribution:** Free (internal tool for IKAROS consultants)

### 3.14 Process Health & Crash Recovery

The `ClaudeSessionManager` must handle unexpected process termination gracefully.

#### Health Monitoring

A background tokio task monitors each active session:

```rust
// Runs every 2 seconds per active session
async fn monitor_session(session_id: String, mut child: Child, app: AppHandle) {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process exited
                let event = if status.success() {
                    "claude:session-ended"
                } else {
                    "claude:session-crashed"
                };
                app.emit(event, SessionEndPayload {
                    session_id: session_id.clone(),
                    exit_code: status.code(),
                    reason: classify_exit(status.code()),
                });
                break;
            }
            Ok(None) => {
                // Still running — check again after delay
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(e) => {
                app.emit("claude:session-crashed", SessionEndPayload {
                    session_id: session_id.clone(),
                    exit_code: None,
                    reason: format!("Monitor error: {e}"),
                });
                break;
            }
        }
    }
}
```

#### Exit Classification

```rust
fn classify_exit(code: Option<i32>) -> String {
    match code {
        Some(0) => "Session ended normally".into(),
        Some(1) => "Claude CLI error (check stderr)".into(),
        Some(137) => "Process killed (OOM or SIGKILL)".into(),
        Some(143) => "Process terminated (SIGTERM)".into(),
        None => "Process terminated by signal".into(),
        Some(c) => format!("Unexpected exit code: {c}"),
    }
}
```

#### UI States

| Event | UI | Action |
|-------|----|--------|
| `claude:session-ended` (exit 0) | "Session ended." | Show restart button |
| `claude:session-crashed` | "Session ended unexpectedly. Restart?" | Show error + restart button |
| `claude:session-crashed` (exit 137) | "Claude ran out of memory. Restart?" | Show error + restart button |

#### Orphan Cleanup

On app startup, the `ClaudeSessionManager` initializes with an empty session map. Since Claude CLI processes are child processes of the Tauri app, they receive SIGTERM when the parent exits normally. For abnormal exits (crash, force-quit):

1. Store active session PIDs in SQLite on spawn
2. On next app startup, check if those PIDs are still alive
3. If alive, send SIGTERM and clean up the DB entries
4. If dead, just clean up the DB entries

#### Broken Pipe Handling

If the stdout pipe breaks (process crashed mid-write):
1. The `BufReader::read_line()` call returns `Ok(0)` (EOF) or `Err`
2. The stream parser task exits its read loop
3. The monitor task detects the process exit via `try_wait()`
4. `claude:session-crashed` is emitted to the UI

### 3.15 Offline Behavior

The app has two modes that behave differently offline:

| Feature | Requires Internet? | Offline behavior |
|---------|-------------------|------------------|
| File browsing | No | Fully functional |
| Task management | No | Fully functional |
| Notes/viewing | No | Fully functional |
| Claude chat | Yes | "Claude requires internet. Check your connection." |
| MCP servers (Gmail, Calendar) | Yes | "Google services unavailable offline." |
| Skill templates | No | Bundled with app binary |
| OAuth login | Yes | "Sign in requires internet." |

On session spawn failure due to network: emit `claude:error` with message "Unable to reach Claude. Check your internet connection and try again." The input bar shows the error inline — no modal dialog.

On mid-session network loss: Claude CLI will emit a `result` event with `subtype: "error"`. The stream parser translates this to `claude:error`. The UI shows: "Connection interrupted. Your work is saved locally." with a retry button.

### 3.16 What This Replaces

| Old (M1) | New (M2) |
|----------|----------|
| `claude_preflight()` — check binary exists | `claude_auth_status()` — check OAuth state |
| `scaffold_claude_project()` — flat project with one CLAUDE.md | `scaffold_engagement()` — 7 skill folders with hierarchical CLAUDE.md |
| `launch_claude()` — open external terminal | `spawn_claude_session()` — headless subprocess, piped to webview |
| `ClaudeView.tsx` — "Open Claude Code" button | `ChatView.tsx` — full embedded chat experience |
| `useClaude.ts` — track PID of external process | `claudeStore.ts` + `useClaudeStream.ts` — real-time streaming state |
| `~/.ikrs-workspace/projects/` — hardcoded path | User-selected workspace folder — no dotfiles outside it |

## 4. Implementation Phases

### Phase 1: Core Subprocess (must work first)
- Rewrite `claude.rs` with `ClaudeSessionManager`
- Stream parser that translates stdout → Tauri events (with hook filtering, unknown event handling)
- Basic `ChatView.tsx` with text streaming
- `claude_auth_status()` and `claude_auth_login()`
- Permission handling: test `--permission-mode default` with stream-json, implement UI approval flow or fall back to `acceptEdits`
- Process health monitoring (try_wait loop, crash recovery)
- Orphan cleanup on app startup

### Phase 2: Skill System
- Skill template files bundled in app binary (8 domains)
- `scaffold_engagement()` with template interpolation
- Orchestrator CLAUDE.md with 8 quality gates
- Skill sync detection and update
- `.skill-version` tracking

### Phase 3: Polished UX + MCP
- `ToolActivityCard.tsx` with collapsible details
- `SessionIndicator.tsx` with cost tracking
- Session resume (`--resume {session_id}`)
- Engagement switching without data loss
- Per-engagement MCP config (`--mcp-config {engagement}/.mcp-config.json`)
- Wire Gmail/Calendar/Drive MCP servers per-engagement credential

### Phase 4: Distribution
- macOS App Sandbox entitlements
- Security-Scoped Bookmarks for workspace folder
- Code signing and notarization
- DMG packaging
- Offline behavior (graceful degradation)

## 5. Risk Register

| ID | Risk | Severity | Likelihood | Mitigation |
|----|------|----------|------------|------------|
| R1 | Claude CLI breaking changes between versions (stream-json format is not a stable API) | HIGH | Medium | Pin minimum CLI version in preflight check. Parser uses catch-all for unknown events. Version tested: v2.1.92. |
| R2 | Read/Write/Edit tools escape engagement folder via absolute paths | MEDIUM | Medium | `--disallowed-tools Bash` removes the worst escape vector. CLAUDE.md instructs boundary. Read/Write with absolute paths is defense-in-depth (advisory). Document as accepted risk. |
| R3 | Process zombie/orphan on app crash | MEDIUM | Medium | Store active PIDs in SQLite. Orphan cleanup on startup. Child processes receive SIGTERM on normal parent exit. |
| R4 | OAuth token expiry during long session | LOW | Low | Claude CLI handles token refresh internally. If refresh fails, CLI exits with error → `claude:session-crashed` → UI shows re-auth prompt. |
| R5 | Cross-engagement data contamination | HIGH | Low | Each session scoped to one engagement folder (cwd). Session manager enforces one-at-a-time (max_sessions=1). Quality gate #8 (Confidentiality) in orchestrator CLAUDE.md. |
| R6 | CLI version incompatibility with future versions | HIGH | Medium | Preflight check: run `claude --version`, compare against minimum required (2.1.92). Show "Please update Claude Code" if below minimum. |
| R7 | Disk space from `~/.claude/` session persistence | LOW | Low | Outside sandbox — Claude CLI manages this. Document as known external data location. Not engagement data but may contain conversation transcripts. |
| R8 | Consultant's global Claude Code hooks fire inside app | MEDIUM | Medium | Hooks are filtered at the parser level (Section 3.2.1). Hooks execute but their output is not shown to the consultant. If hooks cause performance issues, fall back to `--setting-sources project`. |
| R9 | Permission prompts block stdin in headless mode | HIGH | Unknown | Must test in Phase 1. Fallback: `--permission-mode acceptEdits`. See Section 3.5 Implementation Note. |
| R10 | Claude reads sensitive files outside engagement via Read tool | MEDIUM | Low | `--disallowed-tools Bash` blocks shell access. Read/Write/Edit tools with absolute paths are a residual risk — CLAUDE.md boundary instruction is the mitigation. Future: investigate `--add-dir` for restrictive scoping when supported. |

## 6. Resolved Questions

These were originally open questions, now resolved with design decisions:

| # | Question | Resolution | Spec Section |
|---|----------|------------|-------------|
| Q1 | Permission prompts in headless mode | Use `--permission-mode default` with UI approval flow. Fallback to `acceptEdits` if stream-json doesn't surface prompts. | 3.5 |
| Q2 | Session persistence in `~/.claude/` | Accepted. Documented as known external data location. May contain conversation transcripts. | Risk R7 |
| Q3 | MCP servers per-engagement | Yes — Phase 3. Use `--mcp-config` with per-engagement config file generated at session spawn time (not scaffold time — token availability changes between scaffold and spawn). | Phase 3 |
| Q4 | Concurrent sessions | One at a time for M2. `ClaudeSessionManager` enforces `max_sessions=1`. HashMap is forward-compatible. | 3.9 |
| Q5 | Offline behavior | Graceful degradation. Files work offline. Chat requires internet. Clear error states. | 3.15 |
