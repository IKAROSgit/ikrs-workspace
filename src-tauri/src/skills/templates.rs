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

// C2 (Codex condition): Use → (U+2192) NOT — (em dash) in creative template
pub const CREATIVE_TEMPLATE: &str = r#"# Creative Skill

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

    #[test]
    fn test_creative_uses_arrow_not_em_dash() {
        // C2 (Codex condition): creative template must use → not —
        assert!(
            CREATIVE_TEMPLATE.contains("\u{2192}"),
            "Creative template must use → (U+2192)"
        );
        assert!(
            !CREATIVE_TEMPLATE.contains("outline \u{2014} content"),
            "Creative template must NOT use — (em dash) in 'outline → content'"
        );
    }
}
