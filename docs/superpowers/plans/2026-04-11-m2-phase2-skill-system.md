# M2 Embedded Claude — Phase 2: Skill System Implementation Plan

> **STATUS: READY** — Not yet started. Depends on Phase 1 (COMPLETE, commit `0bc4d1b`).
> **Codex review:** 7/10 APPROVED WITH CONDITIONS — C1: path traversal protection, C2: creative template `→` not `—`, C3: semver comparison not string `!=`, C4: useMemo for skillUpdateParams. All must be fixed during execution.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the skill system that makes each engagement a structured AI workspace. Every new engagement gets an orchestrator CLAUDE.md with 8 quality gates and 8 domain skill folders, each with their own CLAUDE.md. Skill templates are versioned and syncable on app update.

**Architecture:** A new `src-tauri/src/skills/` Rust module owns all template content, scaffolding, and sync logic. Templates are hardcoded Rust string constants (bundled in the binary). Variable interpolation uses simple `{braces}` string replacement at scaffold time. Engagement creation in the UI calls `scaffold_engagement_skills` which creates the full folder tree with interpolated CLAUDE.md files and a `.skill-version` JSON marker. On engagement open, `check_skill_updates` compares bundled vs installed template version and reports which folders can be updated.

**Tech Stack:** Rust (serde_json, chrono), Tauri 2.x IPC commands, TypeScript 5, React 19, Zustand v5

**Spec:** `docs/specs/embedded-claude-architecture.md` — Sections 3.6 (Orchestrator), 3.7 (8 Domains), 3.8 (Sync & Evolution), Phase 2 scope (line 945-950)

---

## File Structure

### Files to CREATE

| File | Responsibility |
|------|---------------|
| `src-tauri/src/skills/mod.rs` | Module root — re-exports templates, scaffold, sync |
| `src-tauri/src/skills/templates.rs` | Hardcoded CLAUDE.md template strings for orchestrator + 8 domains, interpolation function |
| `src-tauri/src/skills/scaffold.rs` | `scaffold_engagement_skills()` — creates 8 domain folder trees + CLAUDE.md files + `.skill-version` |
| `src-tauri/src/skills/sync.rs` | `check_skill_updates()` and `apply_skill_updates()` — version comparison, customization detection |
| `src-tauri/src/skills/commands.rs` | Tauri commands wrapping scaffold/check/apply for IPC |
| `src/types/skills.ts` | TypeScript types for skill update status, skill folder info |
| `src/components/skills/SkillStatusPanel.tsx` | UI panel showing skill folder status per engagement with update button |
| `tests/unit/skills-types.test.ts` | TypeScript type guard tests for skill types |

### Files to MODIFY

| File | Change |
|------|--------|
| `src-tauri/src/lib.rs` | Add `mod skills;` and register 3 new commands |
| `src/lib/tauri-commands.ts` | Add `scaffoldEngagementSkills`, `checkSkillUpdates`, `applySkillUpdates` |
| `src/views/SettingsView.tsx` | Call `scaffoldEngagementSkills` after engagement creation, add SkillStatusPanel |

---

## Task 1: Skill Template Constants

**Files:**
- Create: `src-tauri/src/skills/templates.rs`
- Create: `src-tauri/src/skills/mod.rs`

- [ ] **Step 1: Create the skills module directory**

```bash
mkdir -p src-tauri/src/skills
```

- [ ] **Step 2: Write `src-tauri/src/skills/templates.rs`**

This file contains ALL template content as Rust string constants. The orchestrator template comes verbatim from spec section 3.6. The 8 domain templates come verbatim from spec section 3.7.

```rust
/// Current version of bundled skill templates.
/// Bump this when templates change between app releases.
pub const TEMPLATE_VERSION: &str = "1.0.0";

/// The 8 skill domains that ship with every engagement.
pub const SKILL_DOMAINS: &[&str] = &[
    "communications",
    "planning",
    "creative",
    "operations",
    "legal",
    "finance",
    "research",
    "talent",
];

/// Subfolders to create inside each skill domain folder.
/// Maps domain name to list of subfolder names.
pub fn domain_subfolders(domain: &str) -> &[&str] {
    match domain {
        "communications" => &["meetings", "drafts", "final", "templates"],
        "planning" => &["timelines", "budgets", "vendors", "risk", "guests"],
        "creative" => &["briefs", "content", "presentations", "brand"],
        "operations" => &["runsheets", "checklists", "sops", "staffing"],
        "legal" => &["contracts", "permits", "compliance", "templates"],
        "finance" => &["invoices", "expenses", "budgets", "reports"],
        "research" => &["venues", "vendors", "market", "competitors"],
        "talent" => &["shortlists", "riders", "schedules", "contracts"],
        _ => &[],
    }
}

/// Interpolation context for template variables.
pub struct TemplateContext {
    pub client_name: String,
    pub client_slug: String,
    pub engagement_title: String,
    pub engagement_description: String,
    pub consultant_name: String,
    pub consultant_email: String,
    pub timezone: String,
    pub start_date: String,
}

/// Simple brace interpolation: replaces `{key}` with value.
/// Not a full template engine — just string replacement for known keys.
pub fn interpolate(template: &str, ctx: &TemplateContext) -> String {
    template
        .replace("{client_name}", &ctx.client_name)
        .replace("{client_slug}", &ctx.client_slug)
        .replace("{engagement_title}", &ctx.engagement_title)
        .replace("{engagement_description}", &ctx.engagement_description)
        .replace("{consultant_name}", &ctx.consultant_name)
        .replace("{consultant_email}", &ctx.consultant_email)
        .replace("{timezone}", &ctx.timezone)
        .replace("{start_date}", &ctx.start_date)
}

/// Orchestrator CLAUDE.md — placed at the engagement root.
/// Source: Spec section 3.6 (verbatim).
pub const ORCHESTRATOR_TEMPLATE: &str = r#"# {client_name} — {engagement_title}

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
"#;

// ─── Domain Templates ──────────────────────────────────────────────────────────
// Source: Spec section 3.7 (verbatim for each domain).

pub const COMMUNICATIONS_TEMPLATE: &str = r#"# Communications Skill

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
"#;

pub const PLANNING_TEMPLATE: &str = r#"# Planning Skill

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
"#;

pub const CREATIVE_TEMPLATE: &str = r#"# Creative Skill

## Capabilities
- Event concepts and mood boards (text descriptions)
- Design briefs for external designers
- Presentation decks (markdown outline — content)
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
"#;

pub const OPERATIONS_TEMPLATE: &str = r#"# Operations Skill

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
"#;

pub const LEGAL_TEMPLATE: &str = r#"# Legal Skill

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
"#;

pub const FINANCE_TEMPLATE: &str = r#"# Finance Skill

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
"#;

pub const RESEARCH_TEMPLATE: &str = r#"# Research Skill

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
"#;

pub const TALENT_TEMPLATE: &str = r#"# Talent & Entertainment Skill

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
"#;

/// Returns the correct template for a given domain name.
pub fn domain_template(domain: &str) -> Option<&'static str> {
    match domain {
        "communications" => Some(COMMUNICATIONS_TEMPLATE),
        "planning" => Some(PLANNING_TEMPLATE),
        "creative" => Some(CREATIVE_TEMPLATE),
        "operations" => Some(OPERATIONS_TEMPLATE),
        "legal" => Some(LEGAL_TEMPLATE),
        "finance" => Some(FINANCE_TEMPLATE),
        "research" => Some(RESEARCH_TEMPLATE),
        "talent" => Some(TALENT_TEMPLATE),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolate_replaces_all_vars() {
        let ctx = TemplateContext {
            client_name: "Al Habtoor".to_string(),
            client_slug: "al-habtoor".to_string(),
            engagement_title: "Annual Gala 2026".to_string(),
            engagement_description: "Black-tie gala dinner for 500 guests".to_string(),
            consultant_name: "Sara Ahmed".to_string(),
            consultant_email: "sara@ikaros.ae".to_string(),
            timezone: "Asia/Dubai".to_string(),
            start_date: "2026-04-11".to_string(),
        };

        let result = interpolate(ORCHESTRATOR_TEMPLATE, &ctx);
        assert!(result.contains("Al Habtoor"));
        assert!(result.contains("Annual Gala 2026"));
        assert!(result.contains("Sara Ahmed"));
        assert!(result.contains("sara@ikaros.ae"));
        assert!(result.contains("Asia/Dubai"));
        assert!(result.contains("2026-04-11"));
        assert!(!result.contains("{client_name}"));
        assert!(!result.contains("{consultant_name}"));
    }

    #[test]
    fn test_interpolate_communications_has_consultant_name() {
        let ctx = TemplateContext {
            client_name: "TestClient".to_string(),
            client_slug: "test-client".to_string(),
            engagement_title: "Test".to_string(),
            engagement_description: "Test engagement".to_string(),
            consultant_name: "Jane Doe".to_string(),
            consultant_email: "jane@test.com".to_string(),
            timezone: "UTC".to_string(),
            start_date: "2026-01-01".to_string(),
        };

        let result = interpolate(COMMUNICATIONS_TEMPLATE, &ctx);
        assert!(result.contains("Jane Doe"));
        assert!(!result.contains("{consultant_name}"));
    }

    #[test]
    fn test_interpolate_finance_has_client_slug() {
        let ctx = TemplateContext {
            client_name: "TestClient".to_string(),
            client_slug: "test-client".to_string(),
            engagement_title: "Test".to_string(),
            engagement_description: "Test engagement".to_string(),
            consultant_name: "Jane Doe".to_string(),
            consultant_email: "jane@test.com".to_string(),
            timezone: "UTC".to_string(),
            start_date: "2026-01-01".to_string(),
        };

        let result = interpolate(FINANCE_TEMPLATE, &ctx);
        assert!(result.contains("test-client-INV-YYYYMMDD-NNN"));
        assert!(!result.contains("{client_slug}"));
    }

    #[test]
    fn test_all_domains_have_templates() {
        for domain in SKILL_DOMAINS {
            assert!(
                domain_template(domain).is_some(),
                "Missing template for domain: {domain}"
            );
        }
    }

    #[test]
    fn test_all_domains_have_subfolders() {
        for domain in SKILL_DOMAINS {
            let folders = domain_subfolders(domain);
            assert!(
                !folders.is_empty(),
                "No subfolders defined for domain: {domain}"
            );
        }
    }

    #[test]
    fn test_orchestrator_has_all_8_quality_gates() {
        let gates = [
            "Accuracy", "Completeness", "Brand Voice", "Scope",
            "Assumptions", "File Hygiene", "Self-Review", "Confidentiality",
        ];
        for gate in &gates {
            assert!(
                ORCHESTRATOR_TEMPLATE.contains(gate),
                "Orchestrator template missing quality gate: {gate}"
            );
        }
    }

    #[test]
    fn test_orchestrator_references_all_8_domains() {
        for domain in SKILL_DOMAINS {
            assert!(
                ORCHESTRATOR_TEMPLATE.contains(&format!("`{domain}/`")),
                "Orchestrator template missing domain reference: {domain}"
            );
        }
    }
}
```

- [ ] **Step 3: Write `src-tauri/src/skills/mod.rs`**

```rust
pub mod templates;
pub mod scaffold;
pub mod sync;
pub mod commands;
```

- [ ] **Step 4: Verify templates module compiles**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && cargo check 2>&1 | tail -20`

This will fail until `scaffold.rs`, `sync.rs`, and `commands.rs` exist, so create empty stubs:

```rust
// scaffold.rs — stub
// sync.rs — stub
// commands.rs — stub
```

Actually, do NOT stub. Instead, comment out the three imports in `mod.rs` temporarily and only include `templates`:

```rust
pub mod templates;
// pub mod scaffold;  // Task 2
// pub mod sync;      // Task 3
// pub mod commands;  // Task 4
```

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && cargo check 2>&1 | tail -20`
Expected: Compiles (may warn about unused module since nothing imports it yet).

- [ ] **Step 5: Run template tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && cargo test --lib skills::templates 2>&1 | tail -30`
Expected: All 7 tests pass.

- [ ] **Step 6: Commit**

```bash
cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace
git add src-tauri/src/skills/templates.rs src-tauri/src/skills/mod.rs
git commit -m "feat(skills): add skill template constants — orchestrator + 8 domains from spec 3.6-3.7"
```

---

## Task 2: Engagement Scaffolder

**Files:**
- Create: `src-tauri/src/skills/scaffold.rs`

- [ ] **Step 1: Write `src-tauri/src/skills/scaffold.rs`**

This creates the full engagement folder tree: orchestrator CLAUDE.md at root, 8 domain folders each with CLAUDE.md and subfolders, plus `.skill-version` marker.

```rust
use crate::skills::templates::{
    self, domain_subfolders, domain_template, interpolate, TemplateContext,
    ORCHESTRATOR_TEMPLATE, SKILL_DOMAINS, TEMPLATE_VERSION,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// The `.skill-version` file content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersion {
    pub template_version: String,
    pub scaffolded_at: String,
    pub customized_folders: Vec<String>,
}

/// Parameters for scaffolding engagement skills.
/// These map to the TemplateContext fields.
#[derive(Debug, Deserialize)]
pub struct ScaffoldParams {
    pub engagement_path: String,
    pub client_name: String,
    pub client_slug: String,
    pub engagement_title: String,
    pub engagement_description: String,
    pub consultant_name: String,
    pub consultant_email: String,
    pub timezone: String,
}

/// Scaffold all skill folders and CLAUDE.md files for an engagement.
///
/// Creates:
/// - `{engagement_path}/CLAUDE.md` (orchestrator)
/// - `{engagement_path}/{domain}/CLAUDE.md` for each of 8 domains
/// - `{engagement_path}/{domain}/{subfolder}/` for each domain's subfolders
/// - `{engagement_path}/.skill-version`
///
/// Idempotent: skips files/dirs that already exist.
pub fn scaffold_engagement_skills(params: &ScaffoldParams) -> Result<String, String> {
    let base = Path::new(&params.engagement_path);

    if !base.exists() {
        fs::create_dir_all(base)
            .map_err(|e| format!("Failed to create engagement dir: {e}"))?;
    }

    let now = chrono::Utc::now().to_rfc3339();
    let ctx = TemplateContext {
        client_name: params.client_name.clone(),
        client_slug: params.client_slug.clone(),
        engagement_title: params.engagement_title.clone(),
        engagement_description: params.engagement_description.clone(),
        consultant_name: params.consultant_name.clone(),
        consultant_email: params.consultant_email.clone(),
        timezone: params.timezone.clone(),
        start_date: now.split('T').next().unwrap_or(&now).to_string(),
    };

    // 1. Write orchestrator CLAUDE.md at root
    let orchestrator_path = base.join("CLAUDE.md");
    if !orchestrator_path.exists() {
        let content = interpolate(ORCHESTRATOR_TEMPLATE, &ctx);
        fs::write(&orchestrator_path, content)
            .map_err(|e| format!("Failed to write orchestrator CLAUDE.md: {e}"))?;
    }

    // 2. Create each domain folder, its CLAUDE.md, and subfolders
    for domain in SKILL_DOMAINS {
        let domain_dir = base.join(domain);
        fs::create_dir_all(&domain_dir)
            .map_err(|e| format!("Failed to create {domain}/ dir: {e}"))?;

        // Domain CLAUDE.md
        let claude_path = domain_dir.join("CLAUDE.md");
        if !claude_path.exists() {
            if let Some(template) = domain_template(domain) {
                let content = interpolate(template, &ctx);
                fs::write(&claude_path, content)
                    .map_err(|e| format!("Failed to write {domain}/CLAUDE.md: {e}"))?;
            }
        }

        // Subfolders
        for subfolder in domain_subfolders(domain) {
            let sub_path = domain_dir.join(subfolder);
            if !sub_path.exists() {
                fs::create_dir_all(&sub_path)
                    .map_err(|e| format!("Failed to create {domain}/{subfolder}/: {e}"))?;
            }
        }
    }

    // 3. Write .skill-version
    let version_path = base.join(".skill-version");
    if !version_path.exists() {
        let version = SkillVersion {
            template_version: TEMPLATE_VERSION.to_string(),
            scaffolded_at: now,
            customized_folders: vec![],
        };
        let json = serde_json::to_string_pretty(&version)
            .map_err(|e| format!("Failed to serialize .skill-version: {e}"))?;
        fs::write(&version_path, json)
            .map_err(|e| format!("Failed to write .skill-version: {e}"))?;
    }

    Ok(params.engagement_path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_temp_dir() -> String {
        let dir = std::env::temp_dir()
            .join(format!("ikrs-test-scaffold-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().to_string()
    }

    #[test]
    fn test_scaffold_creates_all_files() {
        let tmp = make_temp_dir();
        let engagement_path = format!("{tmp}/test-engagement");

        let params = ScaffoldParams {
            engagement_path: engagement_path.clone(),
            client_name: "Test Corp".to_string(),
            client_slug: "test-corp".to_string(),
            engagement_title: "Annual Event".to_string(),
            engagement_description: "Test event".to_string(),
            consultant_name: "Sara Ahmed".to_string(),
            consultant_email: "sara@ikaros.ae".to_string(),
            timezone: "Asia/Dubai".to_string(),
        };

        let result = scaffold_engagement_skills(&params);
        assert!(result.is_ok());

        // Orchestrator CLAUDE.md exists
        let orchestrator = std::path::Path::new(&engagement_path).join("CLAUDE.md");
        assert!(orchestrator.exists(), "Missing orchestrator CLAUDE.md");
        let content = fs::read_to_string(&orchestrator).unwrap();
        assert!(content.contains("Test Corp"), "Orchestrator missing client name");
        assert!(content.contains("Sara Ahmed"), "Orchestrator missing consultant name");
        assert!(content.contains("Quality Gates"), "Orchestrator missing quality gates");

        // All 8 domain folders exist with CLAUDE.md
        for domain in templates::SKILL_DOMAINS {
            let domain_claude = std::path::Path::new(&engagement_path)
                .join(domain)
                .join("CLAUDE.md");
            assert!(
                domain_claude.exists(),
                "Missing {domain}/CLAUDE.md"
            );
        }

        // .skill-version exists and is valid JSON
        let version_path = std::path::Path::new(&engagement_path).join(".skill-version");
        assert!(version_path.exists(), "Missing .skill-version");
        let version_json = fs::read_to_string(&version_path).unwrap();
        let version: SkillVersion = serde_json::from_str(&version_json).unwrap();
        assert_eq!(version.template_version, templates::TEMPLATE_VERSION);
        assert!(version.customized_folders.is_empty());

        // Spot-check subfolders
        assert!(std::path::Path::new(&engagement_path).join("communications/meetings").exists());
        assert!(std::path::Path::new(&engagement_path).join("planning/timelines").exists());
        assert!(std::path::Path::new(&engagement_path).join("finance/invoices").exists());
        assert!(std::path::Path::new(&engagement_path).join("talent/riders").exists());

        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_scaffold_is_idempotent() {
        let tmp = make_temp_dir();
        let engagement_path = format!("{tmp}/idempotent-test");

        let params = ScaffoldParams {
            engagement_path: engagement_path.clone(),
            client_name: "Idem Corp".to_string(),
            client_slug: "idem-corp".to_string(),
            engagement_title: "Event".to_string(),
            engagement_description: "Test".to_string(),
            consultant_name: "Test User".to_string(),
            consultant_email: "test@test.com".to_string(),
            timezone: "UTC".to_string(),
        };

        // Run twice
        scaffold_engagement_skills(&params).unwrap();

        // Modify orchestrator CLAUDE.md (simulating user customization)
        let orchestrator = std::path::Path::new(&engagement_path).join("CLAUDE.md");
        fs::write(&orchestrator, "# Custom content").unwrap();

        // Run again — should NOT overwrite customized file
        scaffold_engagement_skills(&params).unwrap();
        let content = fs::read_to_string(&orchestrator).unwrap();
        assert_eq!(content, "# Custom content", "Scaffold overwrote existing file!");

        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }
}
```

- [ ] **Step 2: Uncomment `scaffold` in `mod.rs`**

Update `src-tauri/src/skills/mod.rs`:

```rust
pub mod templates;
pub mod scaffold;
// pub mod sync;      // Task 3
// pub mod commands;  // Task 4
```

- [ ] **Step 3: Verify compilation**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && cargo check 2>&1 | tail -20`
Expected: Compiles.

- [ ] **Step 4: Run scaffold tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && cargo test --lib skills::scaffold 2>&1 | tail -30`
Expected: Both tests pass.

- [ ] **Step 5: Commit**

```bash
cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace
git add src-tauri/src/skills/scaffold.rs src-tauri/src/skills/mod.rs
git commit -m "feat(skills): add engagement scaffolder — creates 8 domain folders with CLAUDE.md + .skill-version"
```

---

## Task 3: Skill Sync & Version Detection

**Files:**
- Create: `src-tauri/src/skills/sync.rs`

- [ ] **Step 1: Write `src-tauri/src/skills/sync.rs`**

This implements the sync logic from spec section 3.8. It compares the bundled template version against `.skill-version` in an engagement, detects which CLAUDE.md files have been customized (modified from bundled defaults), and can apply updates to non-customized folders.

```rust
use crate::skills::scaffold::SkillVersion;
use crate::skills::templates::{
    self, domain_template, interpolate, TemplateContext, SKILL_DOMAINS, TEMPLATE_VERSION,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Status of skill updates for an engagement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUpdateStatus {
    /// Whether updates are available (bundled version > installed version).
    pub updates_available: bool,
    /// Bundled template version in the app binary.
    pub bundled_version: String,
    /// Installed template version in the engagement folder.
    pub installed_version: String,
    /// Folders that can be updated (not customized by the user).
    pub updatable_folders: Vec<String>,
    /// Folders the user has customized (CLAUDE.md content differs from default).
    pub customized_folders: Vec<String>,
    /// Folders explicitly tracked as customized in `.skill-version`.
    pub user_marked_custom: Vec<String>,
}

/// Check if skill template updates are available for an engagement.
///
/// Algorithm (from spec 3.8):
/// 1. Read .skill-version from engagement folder
/// 2. Compare with bundled template version
/// 3. If bundled > engagement:
///    a. Check which folders have been customized (modified CLAUDE.md)
///    b. Report updatable vs customized folders
/// 4. If equal: no updates
pub fn check_skill_updates(
    engagement_path: &str,
    ctx: &TemplateContext,
) -> Result<SkillUpdateStatus, String> {
    let base = Path::new(engagement_path);
    let version_path = base.join(".skill-version");

    if !version_path.exists() {
        return Err("No .skill-version found — engagement may not be scaffolded".to_string());
    }

    let version_json = fs::read_to_string(&version_path)
        .map_err(|e| format!("Failed to read .skill-version: {e}"))?;
    let version: SkillVersion = serde_json::from_str(&version_json)
        .map_err(|e| format!("Failed to parse .skill-version: {e}"))?;

    let updates_available = TEMPLATE_VERSION != version.template_version;

    let mut updatable_folders = Vec::new();
    let mut customized_folders = Vec::new();

    if updates_available {
        for domain in SKILL_DOMAINS {
            let claude_path = base.join(domain).join("CLAUDE.md");

            // If user explicitly marked this folder as custom, skip it
            if version.customized_folders.contains(&domain.to_string()) {
                customized_folders.push(domain.to_string());
                continue;
            }

            if claude_path.exists() {
                // Compare current content with what the PREVIOUS version would have generated.
                // Since we only have the current bundled templates, we check if the file
                // matches the current bundled template (un-interpolated vars replaced).
                // If it doesn't match, the user customized it.
                if let Some(template) = domain_template(domain) {
                    let expected = interpolate(template, ctx);
                    let actual = fs::read_to_string(&claude_path)
                        .map_err(|e| format!("Failed to read {domain}/CLAUDE.md: {e}"))?;

                    if actual.trim() == expected.trim() {
                        updatable_folders.push(domain.to_string());
                    } else {
                        customized_folders.push(domain.to_string());
                    }
                }
            } else {
                // CLAUDE.md doesn't exist — can be created fresh
                updatable_folders.push(domain.to_string());
            }
        }
    }

    Ok(SkillUpdateStatus {
        updates_available,
        bundled_version: TEMPLATE_VERSION.to_string(),
        installed_version: version.template_version.clone(),
        updatable_folders,
        customized_folders,
        user_marked_custom: version.customized_folders.clone(),
    })
}

/// Apply skill updates to specified folders.
///
/// Only updates CLAUDE.md in folders explicitly listed in `folders_to_update`.
/// Updates `.skill-version` to the bundled version after applying.
/// Does NOT touch folders not in the list — the UI decides which to update.
pub fn apply_skill_updates(
    engagement_path: &str,
    folders_to_update: &[String],
    ctx: &TemplateContext,
) -> Result<(), String> {
    let base = Path::new(engagement_path);
    let version_path = base.join(".skill-version");

    // Validate that all requested folders are valid domains
    for folder in folders_to_update {
        if !SKILL_DOMAINS.contains(&folder.as_str()) {
            return Err(format!("Unknown skill domain: {folder}"));
        }
    }

    // Update each requested folder's CLAUDE.md
    for folder in folders_to_update {
        if let Some(template) = domain_template(folder) {
            let content = interpolate(template, ctx);
            let domain_dir = base.join(folder);
            fs::create_dir_all(&domain_dir)
                .map_err(|e| format!("Failed to create {folder}/ dir: {e}"))?;
            fs::write(domain_dir.join("CLAUDE.md"), content)
                .map_err(|e| format!("Failed to write {folder}/CLAUDE.md: {e}"))?;
        }
    }

    // Also update the orchestrator CLAUDE.md if it exists and hasn't been customized
    // (The orchestrator is not in the folders_to_update list — it's always updated
    //  unless the user explicitly customized it. For now we leave it alone;
    //  future version may add orchestrator to the sync UI.)

    // Update .skill-version
    let version_json = fs::read_to_string(&version_path)
        .map_err(|e| format!("Failed to read .skill-version: {e}"))?;
    let mut version: SkillVersion = serde_json::from_str(&version_json)
        .map_err(|e| format!("Failed to parse .skill-version: {e}"))?;

    version.template_version = TEMPLATE_VERSION.to_string();
    let json = serde_json::to_string_pretty(&version)
        .map_err(|e| format!("Failed to serialize .skill-version: {e}"))?;
    fs::write(&version_path, json)
        .map_err(|e| format!("Failed to write .skill-version: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::scaffold::{scaffold_engagement_skills, ScaffoldParams};

    fn make_temp_dir() -> String {
        let dir = std::env::temp_dir()
            .join(format!("ikrs-test-sync-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().to_string()
    }

    fn test_ctx() -> TemplateContext {
        TemplateContext {
            client_name: "Sync Corp".to_string(),
            client_slug: "sync-corp".to_string(),
            engagement_title: "Test Event".to_string(),
            engagement_description: "Sync test".to_string(),
            consultant_name: "Sara Ahmed".to_string(),
            consultant_email: "sara@ikaros.ae".to_string(),
            timezone: "Asia/Dubai".to_string(),
            start_date: "2026-04-11".to_string(),
        }
    }

    #[test]
    fn test_check_no_updates_when_versions_match() {
        let tmp = make_temp_dir();
        let path = format!("{tmp}/check-match");
        let ctx = test_ctx();

        let params = ScaffoldParams {
            engagement_path: path.clone(),
            client_name: ctx.client_name.clone(),
            client_slug: ctx.client_slug.clone(),
            engagement_title: ctx.engagement_title.clone(),
            engagement_description: ctx.engagement_description.clone(),
            consultant_name: ctx.consultant_name.clone(),
            consultant_email: ctx.consultant_email.clone(),
            timezone: ctx.timezone.clone(),
        };
        scaffold_engagement_skills(&params).unwrap();

        let status = check_skill_updates(&path, &ctx).unwrap();
        assert!(!status.updates_available);
        assert_eq!(status.bundled_version, status.installed_version);
        assert!(status.updatable_folders.is_empty());
        assert!(status.customized_folders.is_empty());

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_check_detects_customized_folder() {
        let tmp = make_temp_dir();
        let path = format!("{tmp}/check-custom");
        let ctx = test_ctx();

        let params = ScaffoldParams {
            engagement_path: path.clone(),
            client_name: ctx.client_name.clone(),
            client_slug: ctx.client_slug.clone(),
            engagement_title: ctx.engagement_title.clone(),
            engagement_description: ctx.engagement_description.clone(),
            consultant_name: ctx.consultant_name.clone(),
            consultant_email: ctx.consultant_email.clone(),
            timezone: ctx.timezone.clone(),
        };
        scaffold_engagement_skills(&params).unwrap();

        // Simulate a version bump by editing .skill-version to an older version
        let version_path = std::path::Path::new(&path).join(".skill-version");
        let mut version: SkillVersion =
            serde_json::from_str(&fs::read_to_string(&version_path).unwrap()).unwrap();
        version.template_version = "0.9.0".to_string();
        fs::write(&version_path, serde_json::to_string_pretty(&version).unwrap()).unwrap();

        // Customize the legal CLAUDE.md
        let legal_claude = std::path::Path::new(&path).join("legal/CLAUDE.md");
        fs::write(&legal_claude, "# My custom legal notes").unwrap();

        let status = check_skill_updates(&path, &ctx).unwrap();
        assert!(status.updates_available);
        assert!(status.customized_folders.contains(&"legal".to_string()));
        assert!(!status.updatable_folders.contains(&"legal".to_string()));
        // Other 7 domains should be updatable
        assert_eq!(status.updatable_folders.len(), 7);

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_apply_updates_only_requested_folders() {
        let tmp = make_temp_dir();
        let path = format!("{tmp}/apply-test");
        let ctx = test_ctx();

        let params = ScaffoldParams {
            engagement_path: path.clone(),
            client_name: ctx.client_name.clone(),
            client_slug: ctx.client_slug.clone(),
            engagement_title: ctx.engagement_title.clone(),
            engagement_description: ctx.engagement_description.clone(),
            consultant_name: ctx.consultant_name.clone(),
            consultant_email: ctx.consultant_email.clone(),
            timezone: ctx.timezone.clone(),
        };
        scaffold_engagement_skills(&params).unwrap();

        // Downgrade version
        let version_path = std::path::Path::new(&path).join(".skill-version");
        let mut version: SkillVersion =
            serde_json::from_str(&fs::read_to_string(&version_path).unwrap()).unwrap();
        version.template_version = "0.9.0".to_string();
        fs::write(&version_path, serde_json::to_string_pretty(&version).unwrap()).unwrap();

        // Apply only to communications and planning
        apply_skill_updates(
            &path,
            &["communications".to_string(), "planning".to_string()],
            &ctx,
        )
        .unwrap();

        // Version should be updated
        let updated: SkillVersion =
            serde_json::from_str(&fs::read_to_string(&version_path).unwrap()).unwrap();
        assert_eq!(updated.template_version, TEMPLATE_VERSION);

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_apply_rejects_unknown_domain() {
        let tmp = make_temp_dir();
        let path = format!("{tmp}/reject-test");
        let ctx = test_ctx();

        let params = ScaffoldParams {
            engagement_path: path.clone(),
            client_name: ctx.client_name.clone(),
            client_slug: ctx.client_slug.clone(),
            engagement_title: ctx.engagement_title.clone(),
            engagement_description: ctx.engagement_description.clone(),
            consultant_name: ctx.consultant_name.clone(),
            consultant_email: ctx.consultant_email.clone(),
            timezone: ctx.timezone.clone(),
        };
        scaffold_engagement_skills(&params).unwrap();

        let result = apply_skill_updates(
            &path,
            &["nonexistent-domain".to_string()],
            &ctx,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown skill domain"));

        fs::remove_dir_all(&tmp).ok();
    }
}
```

- [ ] **Step 2: Uncomment `sync` in `mod.rs`**

Update `src-tauri/src/skills/mod.rs`:

```rust
pub mod templates;
pub mod scaffold;
pub mod sync;
// pub mod commands;  // Task 4
```

- [ ] **Step 3: Verify compilation**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && cargo check 2>&1 | tail -20`
Expected: Compiles.

- [ ] **Step 4: Run sync tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && cargo test --lib skills::sync 2>&1 | tail -30`
Expected: All 4 tests pass.

- [ ] **Step 5: Commit**

```bash
cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace
git add src-tauri/src/skills/sync.rs src-tauri/src/skills/mod.rs
git commit -m "feat(skills): add skill sync — version detection, customization check, selective updates"
```

---

## Task 4: Tauri IPC Commands

**Files:**
- Create: `src-tauri/src/skills/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write `src-tauri/src/skills/commands.rs`**

These are the Tauri command wrappers that the React frontend calls via `invoke()`. They translate IPC arguments into the internal scaffold/sync API.

```rust
use crate::skills::scaffold::{scaffold_engagement_skills, ScaffoldParams};
use crate::skills::sync::{apply_skill_updates, check_skill_updates, SkillUpdateStatus};
use crate::skills::templates::TemplateContext;

/// Scaffold skill folders for a new engagement.
///
/// Called from React when creating a new engagement.
/// Creates orchestrator CLAUDE.md + 8 domain folders + .skill-version.
#[tauri::command]
pub async fn scaffold_engagement_skills_cmd(
    engagement_path: String,
    client_name: String,
    client_slug: String,
    engagement_title: String,
    engagement_description: String,
    consultant_name: String,
    consultant_email: String,
    timezone: String,
) -> Result<String, String> {
    let params = ScaffoldParams {
        engagement_path,
        client_name,
        client_slug,
        engagement_title,
        engagement_description,
        consultant_name,
        consultant_email,
        timezone,
    };

    // Run filesystem operations on a blocking thread (not the async runtime)
    tokio::task::spawn_blocking(move || scaffold_engagement_skills(&params))
        .await
        .map_err(|e| format!("Scaffold task panicked: {e}"))?
}

/// Check if skill template updates are available for an engagement.
///
/// Called from React when opening an engagement (or from the skill status panel).
/// Returns which folders can be updated vs which have been customized.
#[tauri::command]
pub async fn check_skill_updates_cmd(
    engagement_path: String,
    client_name: String,
    client_slug: String,
    engagement_title: String,
    engagement_description: String,
    consultant_name: String,
    consultant_email: String,
    timezone: String,
    start_date: String,
) -> Result<SkillUpdateStatus, String> {
    let ctx = TemplateContext {
        client_name,
        client_slug,
        engagement_title,
        engagement_description,
        consultant_name,
        consultant_email,
        timezone,
        start_date,
    };

    let path = engagement_path.clone();
    tokio::task::spawn_blocking(move || check_skill_updates(&path, &ctx))
        .await
        .map_err(|e| format!("Check task panicked: {e}"))?
}

/// Apply skill template updates to selected folders.
///
/// Called from React when the user clicks "Update skills" on specific folders.
/// Only updates the folders listed — does not touch customized ones.
#[tauri::command]
pub async fn apply_skill_updates_cmd(
    engagement_path: String,
    folders_to_update: Vec<String>,
    client_name: String,
    client_slug: String,
    engagement_title: String,
    engagement_description: String,
    consultant_name: String,
    consultant_email: String,
    timezone: String,
    start_date: String,
) -> Result<(), String> {
    let ctx = TemplateContext {
        client_name,
        client_slug,
        engagement_title,
        engagement_description,
        consultant_name,
        consultant_email,
        timezone,
        start_date,
    };

    let path = engagement_path.clone();
    let folders = folders_to_update.clone();
    tokio::task::spawn_blocking(move || apply_skill_updates(&path, &folders, &ctx))
        .await
        .map_err(|e| format!("Apply task panicked: {e}"))?
}
```

- [ ] **Step 2: Uncomment `commands` in `mod.rs`**

Update `src-tauri/src/skills/mod.rs` to its final form:

```rust
pub mod templates;
pub mod scaffold;
pub mod sync;
pub mod commands;
```

- [ ] **Step 3: Update `src-tauri/src/lib.rs`**

Add `mod skills;` and register the 3 new commands. The full updated file:

```rust
mod claude;
mod commands;
mod mcp;
mod oauth;
mod skills;

use claude::ClaudeSessionManager;
use mcp::manager::McpProcessManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_sql::Builder::new().build())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_keyring::init())
        .manage(commands::oauth::OAuthState::default())
        .manage(McpProcessManager::new())
        .manage(ClaudeSessionManager::new())
        .invoke_handler(tauri::generate_handler![
            commands::credentials::store_credential,
            commands::credentials::get_credential,
            commands::credentials::delete_credential,
            commands::oauth::start_oauth,
            commands::oauth::exchange_oauth_code,
            commands::mcp::spawn_mcp,
            commands::mcp::kill_mcp,
            commands::mcp::kill_all_mcp,
            commands::mcp::mcp_health,
            commands::mcp::restart_mcp,
            commands::vault::create_vault,
            commands::vault::archive_vault,
            commands::vault::restore_vault,
            commands::vault::delete_vault,
            // Claude M2 — embedded subprocess
            claude::auth::claude_version_check,
            claude::auth::claude_auth_status,
            claude::auth::claude_auth_login,
            claude::commands::spawn_claude_session,
            claude::commands::send_claude_message,
            claude::commands::kill_claude_session,
            // Skills — Phase 2
            skills::commands::scaffold_engagement_skills_cmd,
            skills::commands::check_skill_updates_cmd,
            skills::commands::apply_skill_updates_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 4: Verify full Rust compilation**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && cargo check 2>&1 | tail -20`
Expected: Compiles with no errors.

- [ ] **Step 5: Run ALL skills tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && cargo test --lib skills 2>&1 | tail -40`
Expected: All 13 tests pass (7 template + 2 scaffold + 4 sync).

- [ ] **Step 6: Commit**

```bash
cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace
git add src-tauri/src/skills/commands.rs src-tauri/src/skills/mod.rs src-tauri/src/lib.rs
git commit -m "feat(skills): add Tauri IPC commands — scaffold, check updates, apply updates"
```

---

## Task 5: TypeScript Types & Command Bindings

**Files:**
- Create: `src/types/skills.ts`
- Modify: `src/lib/tauri-commands.ts`
- Create: `tests/unit/skills-types.test.ts`

- [ ] **Step 1: Write `src/types/skills.ts`**

```typescript
/** Status of skill updates for an engagement. Mirrors Rust SkillUpdateStatus. */
export interface SkillUpdateStatus {
  updates_available: boolean;
  bundled_version: string;
  installed_version: string;
  updatable_folders: string[];
  customized_folders: string[];
  user_marked_custom: string[];
}

/** The 8 bundled skill domains. */
export const SKILL_DOMAINS = [
  "communications",
  "planning",
  "creative",
  "operations",
  "legal",
  "finance",
  "research",
  "talent",
] as const;

export type SkillDomain = (typeof SKILL_DOMAINS)[number];

/** Info about a single skill folder in an engagement. */
export interface SkillFolderInfo {
  domain: SkillDomain;
  exists: boolean;
  hasClaudeMd: boolean;
  isCustomized: boolean;
  isUpdatable: boolean;
}

/** Parameters for scaffolding engagement skills. */
export interface ScaffoldSkillsParams {
  engagementPath: string;
  clientName: string;
  clientSlug: string;
  engagementTitle: string;
  engagementDescription: string;
  consultantName: string;
  consultantEmail: string;
  timezone: string;
}

/** Parameters for checking/applying skill updates (adds startDate). */
export interface SkillUpdateParams extends ScaffoldSkillsParams {
  startDate: string;
}

/** Type guard: checks if a string is a valid SkillDomain. */
export function isSkillDomain(value: string): value is SkillDomain {
  return (SKILL_DOMAINS as readonly string[]).includes(value);
}
```

- [ ] **Step 2: Add skill commands to `src/lib/tauri-commands.ts`**

Append the following to the end of the file (after the existing Claude M2 section):

```typescript
// Skills — Phase 2

import type {
  SkillUpdateStatus,
  ScaffoldSkillsParams,
  SkillUpdateParams,
} from "@/types/skills";

export async function scaffoldEngagementSkills(
  params: ScaffoldSkillsParams,
): Promise<string> {
  return invoke("scaffold_engagement_skills_cmd", {
    engagementPath: params.engagementPath,
    clientName: params.clientName,
    clientSlug: params.clientSlug,
    engagementTitle: params.engagementTitle,
    engagementDescription: params.engagementDescription,
    consultantName: params.consultantName,
    consultantEmail: params.consultantEmail,
    timezone: params.timezone,
  });
}

export async function checkSkillUpdates(
  params: SkillUpdateParams,
): Promise<SkillUpdateStatus> {
  return invoke("check_skill_updates_cmd", {
    engagementPath: params.engagementPath,
    clientName: params.clientName,
    clientSlug: params.clientSlug,
    engagementTitle: params.engagementTitle,
    engagementDescription: params.engagementDescription,
    consultantName: params.consultantName,
    consultantEmail: params.consultantEmail,
    timezone: params.timezone,
    startDate: params.startDate,
  });
}

export async function applySkillUpdates(
  params: SkillUpdateParams,
  foldersToUpdate: string[],
): Promise<void> {
  return invoke("apply_skill_updates_cmd", {
    engagementPath: params.engagementPath,
    foldersToUpdate,
    clientName: params.clientName,
    clientSlug: params.clientSlug,
    engagementTitle: params.engagementTitle,
    engagementDescription: params.engagementDescription,
    consultantName: params.consultantName,
    consultantEmail: params.consultantEmail,
    timezone: params.timezone,
    startDate: params.startDate,
  });
}
```

- [ ] **Step 3: Write `tests/unit/skills-types.test.ts`**

```typescript
import { describe, it, expect } from "vitest";
import {
  SKILL_DOMAINS,
  isSkillDomain,
  type SkillUpdateStatus,
  type SkillFolderInfo,
} from "@/types/skills";

describe("SKILL_DOMAINS", () => {
  it("contains exactly 8 domains", () => {
    expect(SKILL_DOMAINS).toHaveLength(8);
  });

  it("contains all expected domains", () => {
    const expected = [
      "communications",
      "planning",
      "creative",
      "operations",
      "legal",
      "finance",
      "research",
      "talent",
    ];
    expect([...SKILL_DOMAINS]).toEqual(expected);
  });
});

describe("isSkillDomain", () => {
  it("returns true for valid domains", () => {
    for (const domain of SKILL_DOMAINS) {
      expect(isSkillDomain(domain)).toBe(true);
    }
  });

  it("returns false for invalid strings", () => {
    expect(isSkillDomain("hospitality")).toBe(false);
    expect(isSkillDomain("")).toBe(false);
    expect(isSkillDomain("COMMUNICATIONS")).toBe(false);
  });
});

describe("SkillUpdateStatus type", () => {
  it("accepts valid status objects", () => {
    const status: SkillUpdateStatus = {
      updates_available: true,
      bundled_version: "1.1.0",
      installed_version: "1.0.0",
      updatable_folders: ["communications", "planning"],
      customized_folders: ["legal"],
      user_marked_custom: [],
    };
    expect(status.updates_available).toBe(true);
    expect(status.updatable_folders).toHaveLength(2);
  });
});

describe("SkillFolderInfo type", () => {
  it("accepts valid folder info", () => {
    const info: SkillFolderInfo = {
      domain: "communications",
      exists: true,
      hasClaudeMd: true,
      isCustomized: false,
      isUpdatable: true,
    };
    expect(info.domain).toBe("communications");
    expect(info.isCustomized).toBe(false);
  });
});
```

- [ ] **Step 4: Run TypeScript typecheck**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx tsc --noEmit 2>&1 | tail -20`
Expected: No errors.

- [ ] **Step 5: Run TypeScript tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run tests/unit/skills-types.test.ts 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace
git add src/types/skills.ts src/lib/tauri-commands.ts tests/unit/skills-types.test.ts
git commit -m "feat(skills): add TypeScript types and Tauri command bindings for skill system"
```

---

## Task 6: Wire Skills into Engagement Creation (SettingsView)

**Files:**
- Modify: `src/views/SettingsView.tsx`

- [ ] **Step 1: Update `handleCreateEngagement` to call `scaffoldEngagementSkills`**

After the engagement is created in Firestore, call the Tauri command to scaffold the skill folders on disk. Update the imports and the handler:

```typescript
import { useState } from "react";
import { homeDir } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { useAuth } from "@/providers/AuthProvider";
import { useEngagementActions } from "@/providers/EngagementProvider";
import { useEngagementStore } from "@/stores/engagementStore";
import { startOAuth, scaffoldEngagementSkills } from "@/lib/tauri-commands";

const GOOGLE_SCOPES = [
  "https://www.googleapis.com/auth/gmail.modify",
  "https://www.googleapis.com/auth/calendar.events",
  "https://www.googleapis.com/auth/drive.file",
];

const OAUTH_CLIENT_ID = import.meta.env.VITE_GOOGLE_OAUTH_CLIENT_ID ?? "";
const OAUTH_PORT = 49152;

export default function SettingsView() {
  const { consultant, logOut } = useAuth();
  const { createClient, createEngagement } = useEngagementActions();
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);

  const [clientName, setClientName] = useState("");
  const [clientDomain, setClientDomain] = useState("");
  const [engagementTitle, setEngagementTitle] = useState("");
  const [engagementDesc, setEngagementDesc] = useState("");
  const [creating, setCreating] = useState(false);
  const [oauthStatus, setOauthStatus] = useState<
    "idle" | "pending" | "success" | "error"
  >("idle");

  const handleCreateEngagement = async () => {
    if (!clientName || !clientDomain || !consultant) return;
    setCreating(true);

    try {
      const slug = clientDomain.replace(/\./g, "-").toLowerCase();
      const home = await homeDir();
      const vaultPath = `${home}.ikrs-workspace/vaults/${slug}/`;

      const clientId = await createClient({
        name: clientName,
        domain: clientDomain,
        slug,
        branding: {},
      });

      const engId = await createEngagement({
        consultantId: consultant.id,
        clientId,
        status: "active",
        startDate: new Date(),
        settings: {
          timezone: consultant.preferences.timezone,
          description: engagementDesc || undefined,
        },
        vault: {
          path: vaultPath,
          status: "active",
        },
      });

      // Scaffold skill folders on disk (Phase 2)
      await scaffoldEngagementSkills({
        engagementPath: vaultPath,
        clientName,
        clientSlug: slug,
        engagementTitle: engagementTitle || `${clientName} Engagement`,
        engagementDescription: engagementDesc || `Engagement for ${clientName}`,
        consultantName: consultant.name,
        consultantEmail: consultant.email,
        timezone: consultant.preferences.timezone,
      });

      useEngagementStore.getState().setActiveEngagement(engId);
      setClientName("");
      setClientDomain("");
      setEngagementTitle("");
      setEngagementDesc("");
    } catch (err) {
      console.error("Failed to create engagement:", err);
    } finally {
      setCreating(false);
    }
  };

  const handleConnectGoogle = async () => {
    if (!activeEngagementId) return;
    setOauthStatus("pending");
    try {
      const { auth_url } = await startOAuth(
        OAUTH_CLIENT_ID,
        OAUTH_PORT,
        GOOGLE_SCOPES,
      );
      await open(auth_url);
      setOauthStatus("success");
    } catch {
      setOauthStatus("error");
    }
  };

  return (
    <div className="flex flex-col gap-6 p-6 max-w-2xl">
      <Card>
        <CardHeader>
          <CardTitle>Profile</CardTitle>
        </CardHeader>
        <CardContent className="space-y-2">
          <p>
            <strong>Name:</strong> {consultant?.name}
          </p>
          <p>
            <strong>Email:</strong> {consultant?.email}
          </p>
          <p>
            <strong>Role:</strong>{" "}
            <Badge variant="secondary">{consultant?.role}</Badge>
          </p>
          <Separator />
          <Button variant="destructive" size="sm" onClick={logOut}>
            Sign out
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>New Engagement</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <Input
            placeholder="Client name (e.g. BLR WORLD)"
            value={clientName}
            onChange={(e) => setClientName(e.target.value)}
          />
          <Input
            placeholder="Client domain (e.g. blr-world.com)"
            value={clientDomain}
            onChange={(e) => setClientDomain(e.target.value)}
          />
          <Input
            placeholder="Engagement title (e.g. Annual Gala 2026)"
            value={engagementTitle}
            onChange={(e) => setEngagementTitle(e.target.value)}
          />
          <Input
            placeholder="Description (optional)"
            value={engagementDesc}
            onChange={(e) => setEngagementDesc(e.target.value)}
          />
          <Button
            onClick={handleCreateEngagement}
            disabled={!clientName || !clientDomain || creating}
          >
            {creating ? "Creating..." : "Create engagement"}
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Google Account</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {!activeEngagementId ? (
            <p className="text-muted-foreground">
              Select an engagement first.
            </p>
          ) : (
            <>
              <Button
                onClick={handleConnectGoogle}
                disabled={oauthStatus === "pending"}
              >
                {oauthStatus === "pending"
                  ? "Connecting..."
                  : "Connect Google Account"}
              </Button>
              {oauthStatus === "success" && (
                <p className="text-green-500 text-sm">
                  Connected successfully.
                </p>
              )}
              {oauthStatus === "error" && (
                <p className="text-red-500 text-sm">
                  Connection failed. Try again.
                </p>
              )}
            </>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
```

Key changes from the existing SettingsView:
- Added import for `scaffoldEngagementSkills`
- Added `engagementTitle` and `engagementDesc` state variables
- Added `creating` loading state
- Added `scaffoldEngagementSkills()` call after Firestore write
- Added two new input fields for engagement title and description
- Added error handling with try/catch/finally
- Disabled button during creation with loading text

- [ ] **Step 2: Verify TypeScript typecheck**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx tsc --noEmit 2>&1 | tail -20`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace
git add src/views/SettingsView.tsx
git commit -m "feat(skills): wire skill scaffolding into engagement creation flow"
```

---

## Task 7: Skill Status Panel Component

**Files:**
- Create: `src/components/skills/SkillStatusPanel.tsx`

- [ ] **Step 1: Create directory**

```bash
mkdir -p src/components/skills
```

- [ ] **Step 2: Write `src/components/skills/SkillStatusPanel.tsx`**

A panel showing skill folder status per engagement, with update detection and an "Update skills" button. Designed to be embedded in SettingsView or a future engagement detail view.

```tsx
import { useState, useEffect, useCallback } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { checkSkillUpdates, applySkillUpdates } from "@/lib/tauri-commands";
import {
  SKILL_DOMAINS,
  type SkillDomain,
  type SkillUpdateStatus,
  type SkillUpdateParams,
} from "@/types/skills";

interface SkillStatusPanelProps {
  /** Parameters needed to check/apply skill updates. */
  updateParams: SkillUpdateParams | null;
}

/** Human-readable labels for each skill domain. */
const DOMAIN_LABELS: Record<SkillDomain, string> = {
  communications: "Communications",
  planning: "Planning",
  creative: "Creative",
  operations: "Operations",
  legal: "Legal",
  finance: "Finance",
  research: "Research",
  talent: "Talent & Entertainment",
};

/** Icons for each domain (unicode/emoji-free, text-based). */
const DOMAIN_ICONS: Record<SkillDomain, string> = {
  communications: "COM",
  planning: "PLN",
  creative: "CRE",
  operations: "OPS",
  legal: "LEG",
  finance: "FIN",
  research: "RES",
  talent: "TAL",
};

export function SkillStatusPanel({ updateParams }: SkillStatusPanelProps) {
  const [status, setStatus] = useState<SkillUpdateStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchStatus = useCallback(async () => {
    if (!updateParams) return;
    setLoading(true);
    setError(null);
    try {
      const result = await checkSkillUpdates(updateParams);
      setStatus(result);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [updateParams]);

  useEffect(() => {
    fetchStatus();
  }, [fetchStatus]);

  const handleUpdateAll = async () => {
    if (!updateParams || !status) return;
    setUpdating(true);
    setError(null);
    try {
      await applySkillUpdates(updateParams, status.updatable_folders);
      await fetchStatus(); // Refresh status after update
    } catch (err) {
      setError(String(err));
    } finally {
      setUpdating(false);
    }
  };

  if (!updateParams) {
    return null;
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between">
          <span>Skill Domains</span>
          {status?.updates_available && (
            <Badge variant="outline" className="text-xs">
              v{status.installed_version} &rarr; v{status.bundled_version}
            </Badge>
          )}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {loading && <p className="text-muted-foreground text-sm">Checking skills...</p>}

        {error && <p className="text-red-500 text-sm">{error}</p>}

        {status && !loading && (
          <>
            <div className="grid grid-cols-2 gap-2">
              {SKILL_DOMAINS.map((domain) => {
                const isCustom = status.customized_folders.includes(domain);
                const isUpdatable = status.updatable_folders.includes(domain);

                return (
                  <div
                    key={domain}
                    className="flex items-center gap-2 p-2 rounded border text-sm"
                  >
                    <span className="font-mono text-xs text-muted-foreground w-8">
                      {DOMAIN_ICONS[domain]}
                    </span>
                    <span className="flex-1">{DOMAIN_LABELS[domain]}</span>
                    {isCustom && (
                      <Badge variant="secondary" className="text-xs">
                        custom
                      </Badge>
                    )}
                    {isUpdatable && status.updates_available && (
                      <Badge variant="default" className="text-xs">
                        update
                      </Badge>
                    )}
                    {!isCustom && !isUpdatable && !status.updates_available && (
                      <Badge variant="outline" className="text-xs">
                        current
                      </Badge>
                    )}
                  </div>
                );
              })}
            </div>

            {status.updates_available && status.updatable_folders.length > 0 && (
              <Button
                onClick={handleUpdateAll}
                disabled={updating}
                size="sm"
                className="w-full"
              >
                {updating
                  ? "Updating..."
                  : `Update ${status.updatable_folders.length} skill${status.updatable_folders.length === 1 ? "" : "s"}`}
              </Button>
            )}

            {status.updates_available && status.customized_folders.length > 0 && (
              <p className="text-muted-foreground text-xs">
                {status.customized_folders.length} skill
                {status.customized_folders.length === 1 ? " has" : "s have"} custom
                CLAUDE.md files and will not be overwritten.
              </p>
            )}

            {!status.updates_available && (
              <p className="text-muted-foreground text-xs">
                All skills are at version {status.installed_version}.
              </p>
            )}
          </>
        )}
      </CardContent>
    </Card>
  );
}
```

- [ ] **Step 3: Verify TypeScript typecheck**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx tsc --noEmit 2>&1 | tail -20`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace
git add src/components/skills/SkillStatusPanel.tsx
git commit -m "feat(skills): add SkillStatusPanel component — shows domain status and update button"
```

---

## Task 8: Integrate SkillStatusPanel into SettingsView

**Files:**
- Modify: `src/views/SettingsView.tsx`

- [ ] **Step 1: Add SkillStatusPanel to SettingsView**

Add the import and render the panel below the New Engagement card, visible when an active engagement is selected. The panel needs the `SkillUpdateParams` which we derive from the active engagement + client + consultant data.

Add this import at the top of SettingsView:

```typescript
import { SkillStatusPanel } from "@/components/skills/SkillStatusPanel";
import type { SkillUpdateParams } from "@/types/skills";
```

Add a derived value inside the component that builds `SkillUpdateParams` from current state:

```typescript
// Derive skill update params from active engagement
const engagements = useEngagementStore((s) => s.engagements);
const clients = useEngagementStore((s) => s.clients);

const skillUpdateParams: SkillUpdateParams | null = (() => {
  if (!activeEngagementId || !consultant) return null;
  const engagement = engagements.find((e) => e.id === activeEngagementId);
  if (!engagement) return null;
  const client = clients.find((c) => c.id === engagement.clientId);
  if (!client) return null;

  return {
    engagementPath: engagement.vault.path,
    clientName: client.name,
    clientSlug: client.slug,
    engagementTitle: engagement.settings.description ?? `${client.name} Engagement`,
    engagementDescription: engagement.settings.description ?? `Engagement for ${client.name}`,
    consultantName: consultant.name,
    consultantEmail: consultant.email,
    timezone: engagement.settings.timezone,
    startDate: engagement.startDate instanceof Date
      ? engagement.startDate.toISOString().split("T")[0]
      : String(engagement.startDate).split("T")[0],
  };
})();
```

Then add the panel in the JSX, after the Google Account card:

```tsx
{activeEngagementId && (
  <SkillStatusPanel updateParams={skillUpdateParams} />
)}
```

The full return JSX should end up as:

```tsx
return (
  <div className="flex flex-col gap-6 p-6 max-w-2xl">
    {/* Profile Card */}
    <Card>...</Card>

    {/* New Engagement Card */}
    <Card>...</Card>

    {/* Google Account Card */}
    <Card>...</Card>

    {/* Skill Status Panel (only when engagement is active) */}
    {activeEngagementId && (
      <SkillStatusPanel updateParams={skillUpdateParams} />
    )}
  </div>
);
```

- [ ] **Step 2: Verify TypeScript typecheck**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx tsc --noEmit 2>&1 | tail -20`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace
git add src/views/SettingsView.tsx
git commit -m "feat(skills): add SkillStatusPanel to SettingsView for active engagement"
```

---

## Task 9: Full Build Verification

**Files:** None (verification only)

- [ ] **Step 1: Run Rust build**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && cargo build 2>&1 | tail -20`
Expected: Build succeeds with no errors.

- [ ] **Step 2: Run ALL Rust tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri && cargo test 2>&1 | tail -40`
Expected: All tests pass (templates: 7, scaffold: 2, sync: 4 = 13 total new tests).

- [ ] **Step 3: Run TypeScript typecheck**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx tsc --noEmit 2>&1 | tail -20`
Expected: No errors.

- [ ] **Step 4: Run ALL TypeScript tests**

Run: `cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace && npx vitest run 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 5: Verify new files exist**

Run:
```bash
echo "=== Rust skills module ===" && \
ls -la /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src-tauri/src/skills/ && \
echo "=== TypeScript types ===" && \
ls -la /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src/types/skills.ts && \
echo "=== UI component ===" && \
ls -la /home/moe_ikaros_ae/projects/apps/ikrs-workspace/src/components/skills/ && \
echo "=== Tests ===" && \
ls -la /home/moe_ikaros_ae/projects/apps/ikrs-workspace/tests/unit/skills-types.test.ts
```

Expected: All files exist:
- `src-tauri/src/skills/mod.rs`
- `src-tauri/src/skills/templates.rs`
- `src-tauri/src/skills/scaffold.rs`
- `src-tauri/src/skills/sync.rs`
- `src-tauri/src/skills/commands.rs`
- `src/types/skills.ts`
- `src/components/skills/SkillStatusPanel.tsx`
- `tests/unit/skills-types.test.ts`

- [ ] **Step 6: Verify no references to old vault scaffolder for skill creation**

Run: `grep -rn "create_vault.*skill\|skill.*create_vault" src/ src-tauri/src/ --include="*.ts" --include="*.tsx" --include="*.rs" 2>&1`
Expected: No matches — vault and skills are separate concerns.

- [ ] **Step 7: Commit if any fixes were needed**

```bash
cd /home/moe_ikaros_ae/projects/apps/ikrs-workspace
git add -A
git commit -m "chore: phase 2 build verification — all clean"
```

---

## Self-Review Checklist

| Spec Section | Task(s) | Covered? |
|-------------|---------|----------|
| 3.6 Orchestrator CLAUDE.md Template (8 quality gates) | T1 | Yes — verbatim from spec, all 8 gates present, test verifies |
| 3.7 Skill Domain Templates (8 domains) | T1 | Yes — all 8 domains verbatim from spec, test verifies all have templates |
| 3.8 Skill Sync & Evolution (.skill-version JSON) | T2, T3 | Yes — SkillVersion struct, check_skill_updates, apply_skill_updates |
| 3.8 Custom folder detection | T3 | Yes — compares content against bundled, respects customized_folders |
| 3.8 Selective update (don't overwrite customized) | T3 | Yes — only updates folders in the explicit list |
| 3.10 scaffold_engagement command | T4 | Yes — scaffold_engagement_skills_cmd Tauri command |
| 3.10 check_skill_updates command | T4 | Yes — check_skill_updates_cmd Tauri command |
| 3.10 sync_skills command | T4 | Yes — apply_skill_updates_cmd Tauri command |
| Phase 2 scope: bundled templates | T1 | Yes — hardcoded Rust constants, compiled into binary |
| Phase 2 scope: template interpolation | T1 | Yes — simple {braces} string replace, tested |
| Phase 2 scope: .skill-version tracking | T2, T3 | Yes — JSON with version, timestamp, customized list |
| Domain subfolders per spec | T2 | Yes — communications/meetings, planning/timelines, etc. |
| UAE/Dubai standards (AED, VAT 5%, DTCM) | T1 | Yes — in finance, legal, planning, talent templates |

**Note:** The vault scaffolder (`commands/vault.rs`) is NOT modified. Skills and vaults are separate concerns. The SettingsView now calls `scaffoldEngagementSkills` after engagement creation, which handles the skill folder tree independently from the Obsidian vault. A future task could unify these into a single `scaffold_engagement` meta-command, but for Phase 2 the separation is cleaner.

---

Plan complete and saved to `docs/superpowers/plans/2026-04-11-m2-phase2-skill-system.md`. Two execution options:

**1. Subagent-Driven (recommended)** - Dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints
