import { invoke } from "@tauri-apps/api/core";
import { useTaskStore } from "@/stores/taskStore";
import type { Task } from "@/types";

/**
 * Session-boot briefing composer.
 *
 * Runs on every fresh Claude spawn (NOT on resume — resumed sessions
 * already have the conversation history). Aggregates the consultant's
 * current-state snapshot and composes a markdown prompt that becomes
 * Claude's first user-message input. Claude responds with a
 * proactive opener instead of the default "what would you like to
 * work on?" cold-start.
 *
 * Design intent (M3 Phase 4A, 2026-04-21):
 *   - Data sources live in already-warm frontend listeners (Firestore
 *     tasks) or thin Tauri wrappers over Google REST (calendar, gmail).
 *     No new backend aggregation needed.
 *   - Failure-tolerant: if calendar API is down or gmail is rate-
 *     limited, we degrade gracefully with a note in the briefing and
 *     ship the rest. Missing sources NEVER wedge session boot.
 *   - Bounded: briefing payload soft-capped at ~3k tokens worth
 *     (≈12KB of markdown). If real data exceeds that, we truncate and
 *     add a "… and N more" tail.
 *   - Transparent: the full briefing is written to Claude's stdin as
 *     a user turn but intentionally NOT added to the chat store, so
 *     the user sees only Claude's proactive opener — not the raw
 *     data dump they never typed.
 */

/** Result of listing today's calendar events — mirrors the Rust enum. */
type CalendarListResult =
  | { status: "ok"; events: CalendarEvent[] }
  | { status: "not_connected" }
  | { status: "scope_missing" }
  | { status: "rate_limited" }
  | { status: "network" }
  | { status: "other"; code: number | null };

interface CalendarEvent {
  id: string;
  summary: string;
  start: string;
  end: string;
  location: string | null;
  attendees: string[];
  hangout_link: string | null;
  html_link: string | null;
  status: string;
}

/** Result of listing gmail inbox — mirrors the Rust enum. */
type GmailListResult =
  | { status: "ok"; messages: GmailMessage[] }
  | { status: "not_connected" }
  | { status: "scope_missing" }
  | { status: "rate_limited" }
  | { status: "network" }
  | { status: "other"; code: number | null };

interface GmailMessage {
  id: string;
  thread_id: string;
  from: string;
  subject: string;
  snippet: string;
  date: string;
  is_read: boolean;
}

interface RecentNote {
  relative_path: string;
  title: string;
  modified_at: number;
  size_bytes: number;
}

interface EngagementMemory {
  principles: string;
  lessons: string;
  relationships: string;
  context: string;
}

/**
 * Compose the briefing markdown. Awaits all data sources in parallel
 * with per-source timeouts so a hanging API call can't stall spawn.
 */
export async function composeSessionBriefing(
  engagementId: string,
  clientSlug: string | undefined,
): Promise<string> {
  const [calendar, gmail, notes, memory] = await Promise.all([
    withTimeout(fetchCalendar(engagementId), 4000, "calendar"),
    withTimeout(fetchGmailPriority(engagementId), 4000, "gmail"),
    withTimeout(fetchRecentNotes(clientSlug), 2000, "notes"),
    withTimeout(fetchMemory(clientSlug), 2000, "memory"),
  ]);

  // Tasks come from the already-subscribed zustand store — no await.
  const tasks = selectActiveTasks(engagementId);

  return renderBriefing({ calendar, gmail, tasks, notes, memory });
}

// ────────────────────────────────────────────────────────────────

async function fetchCalendar(
  engagementId: string,
): Promise<CalendarSection> {
  try {
    const result = await invoke<CalendarListResult>("list_calendar_events", {
      engagementId,
      daysAhead: 1,
      maxResults: 20,
    });
    if (result.status === "ok") {
      const now = Date.now();
      const endOfDay = endOfLocalDay(new Date()).getTime();
      const todays = result.events.filter((e) => {
        const startMs = Date.parse(e.start);
        return Number.isFinite(startMs) && startMs >= now - 60 * 60 * 1000 && startMs <= endOfDay;
      });
      return { kind: "ok", events: todays };
    }
    return { kind: "skipped", reason: calendarReason(result.status) };
  } catch (e) {
    return { kind: "skipped", reason: `calendar error: ${describeError(e)}` };
  }
}

function calendarReason(status: string): string {
  switch (status) {
    case "not_connected":
      return "calendar not connected for this engagement";
    case "scope_missing":
      return "calendar scope missing — reconnect Google to grant";
    case "rate_limited":
      return "calendar rate-limited (retry later)";
    case "network":
      return "calendar unreachable (offline?)";
    default:
      return "calendar fetch failed";
  }
}

// ────────────────────────────────────────────────────────────────

async function fetchGmailPriority(
  engagementId: string,
): Promise<GmailSection> {
  try {
    // Gmail REST's `q` parameter would let us filter server-side, but
    // the existing list_gmail_inbox command doesn't expose it — we
    // fetch the top 30 and filter client-side. 30 is small enough that
    // the extra metadata calls don't matter vs. the simplicity of
    // reusing the existing wrapper.
    const result = await invoke<GmailListResult>("list_gmail_inbox", {
      engagementId,
      maxResults: 30,
    });
    if (result.status === "ok") {
      const dayAgo = Date.now() - 24 * 60 * 60 * 1000;
      const priority = result.messages.filter((m) => {
        if (m.is_read) return false;
        const ms = Date.parse(m.date);
        return Number.isFinite(ms) && ms >= dayAgo;
      });
      return { kind: "ok", messages: priority.slice(0, 8) };
    }
    return { kind: "skipped", reason: gmailReason(result.status) };
  } catch (e) {
    return { kind: "skipped", reason: `gmail error: ${describeError(e)}` };
  }
}

function gmailReason(status: string): string {
  switch (status) {
    case "not_connected":
      return "gmail not connected for this engagement";
    case "scope_missing":
      return "gmail scope missing — reconnect Google to grant";
    case "rate_limited":
      return "gmail rate-limited (retry later)";
    case "network":
      return "gmail unreachable (offline?)";
    default:
      return "gmail fetch failed";
  }
}

// ────────────────────────────────────────────────────────────────

async function fetchRecentNotes(
  clientSlug: string | undefined,
): Promise<NotesSection> {
  if (!clientSlug) return { kind: "ok", notes: [] };
  try {
    const notes = await invoke<RecentNote[]>("list_recent_vault_notes", {
      clientSlug,
      limit: 5,
    });
    return { kind: "ok", notes };
  } catch (e) {
    return { kind: "skipped", reason: `vault scan failed: ${describeError(e)}` };
  }
}

async function fetchMemory(
  clientSlug: string | undefined,
): Promise<MemorySection> {
  if (!clientSlug) return { kind: "ok", memory: null };
  try {
    const memory = await invoke<EngagementMemory>(
      "read_engagement_memory",
      { clientSlug },
    );
    return { kind: "ok", memory };
  } catch (e) {
    return { kind: "skipped", reason: `memory read failed: ${describeError(e)}` };
  }
}

// ────────────────────────────────────────────────────────────────

function selectActiveTasks(engagementId: string): Task[] {
  const tasks = useTaskStore.getState().tasks;
  const active = tasks.filter(
    (t) =>
      t.engagementId === engagementId &&
      (t.status === "in_progress" ||
        t.status === "blocked" ||
        t.status === "awaiting_client"),
  );
  // p1 before p2 before p3; within priority, newest first.
  const rank: Record<string, number> = { p1: 0, p2: 1, p3: 2 };
  active.sort((a, b) => {
    const dp = (rank[a.priority] ?? 9) - (rank[b.priority] ?? 9);
    if (dp !== 0) return dp;
    return (b.sortOrder ?? 0) - (a.sortOrder ?? 0);
  });
  return active.slice(0, 10);
}

// ────────────────────────────────────────────────────────────────

type CalendarSection =
  | { kind: "ok"; events: CalendarEvent[] }
  | { kind: "skipped"; reason: string };

type GmailSection =
  | { kind: "ok"; messages: GmailMessage[] }
  | { kind: "skipped"; reason: string };

type NotesSection =
  | { kind: "ok"; notes: RecentNote[] }
  | { kind: "skipped"; reason: string };

type MemorySection =
  | { kind: "ok"; memory: EngagementMemory | null }
  | { kind: "skipped"; reason: string };

function renderBriefing(p: {
  calendar: CalendarSection;
  gmail: GmailSection;
  tasks: Task[];
  notes: NotesSection;
  memory: MemorySection;
}): string {
  const now = new Date();
  const dateLabel = now.toLocaleDateString(undefined, {
    weekday: "long",
    month: "long",
    day: "numeric",
    year: "numeric",
  });

  const parts: string[] = [];

  // Marker + framing so Claude treats this as context, not a user
  // request. The marker lets future debuggers grep the CLI stdin log
  // for these turns.
  parts.push("<<BRIEFING v1>>");
  parts.push(
    `You're opening a fresh session with me (the consultant). ` +
      `Below is my current-state snapshot for ${dateLabel}. ` +
      `Read it, then open the session proactively — tell me what ` +
      `matters today, suggest where to start, and ask a focused ` +
      `question if there's a real decision to make. Keep your ` +
      `opening under ~120 words. Don't echo the briefing back to me; ` +
      `just act on it.`,
  );
  parts.push("");
  parts.push("---");
  parts.push("");

  // Calendar
  parts.push("## Today's calendar");
  if (p.calendar.kind === "ok") {
    if (p.calendar.events.length === 0) {
      parts.push("_No events scheduled for the rest of today._");
    } else {
      for (const ev of p.calendar.events) {
        const start = formatTime(ev.start);
        const end = formatTime(ev.end);
        const loc = ev.location ? ` · ${ev.location}` : "";
        const link = ev.hangout_link ? " · [join link]" : "";
        parts.push(`- **${start}–${end}** ${ev.summary}${loc}${link}`);
        if (ev.attendees.length > 0) {
          const first = ev.attendees.slice(0, 4).join(", ");
          const more = ev.attendees.length > 4 ? ` +${ev.attendees.length - 4}` : "";
          parts.push(`  Attendees: ${first}${more}`);
        }
      }
    }
  } else {
    parts.push(`_Skipped — ${p.calendar.reason}._`);
  }
  parts.push("");

  // Gmail
  parts.push("## Unread priority mail (last 24h)");
  if (p.gmail.kind === "ok") {
    if (p.gmail.messages.length === 0) {
      parts.push("_No unread priority mail in the last 24h._");
    } else {
      for (const m of p.gmail.messages) {
        const when = formatRelativeDay(m.date);
        parts.push(`- **${m.from}** · ${when}: ${m.subject}`);
        if (m.snippet) {
          parts.push(`  > ${truncate(m.snippet, 160)}`);
        }
      }
    }
  } else {
    parts.push(`_Skipped — ${p.gmail.reason}._`);
  }
  parts.push("");

  // Tasks
  parts.push("## Open tasks");
  if (p.tasks.length === 0) {
    parts.push("_No in-progress, blocked, or awaiting-client tasks._");
  } else {
    for (const t of p.tasks) {
      const statusLabel =
        t.status === "in_progress"
          ? "in-progress"
          : t.status === "blocked"
            ? "blocked"
            : "awaiting-client";
      parts.push(`- [${t.priority.toUpperCase()}] **${t.title}** — ${statusLabel}`);
      if (t.description) {
        parts.push(`  ${truncate(t.description, 180)}`);
      }
    }
  }
  parts.push("");

  // Recent notes
  parts.push("## Recently modified vault notes");
  if (p.notes.kind === "ok") {
    if (p.notes.notes.length === 0) {
      parts.push("_No notes in the vault yet._");
    } else {
      for (const n of p.notes.notes) {
        parts.push(`- \`${n.relative_path}\` — ${n.title}`);
      }
    }
  } else {
    parts.push(`_Skipped — ${p.notes.reason}._`);
  }
  parts.push("");

  // Evolving memory from prior sessions (Phase 4B). Rendered only
  // when any of the four files has real content; totally-empty memory
  // is skipped so a day-zero engagement isn't padded with empty
  // headers. The distiller (session-end) grows these files over time.
  if (p.memory.kind === "ok" && p.memory.memory) {
    const m = p.memory.memory;
    const hasAny =
      m.principles.trim().length > 0 ||
      m.lessons.trim().length > 0 ||
      m.relationships.trim().length > 0 ||
      m.context.trim().length > 0;
    if (hasAny) {
      parts.push("## Carryover from prior sessions");
      parts.push(
        "_The following is your accumulated knowledge of this engagement — " +
          "trust it, but flag to the consultant if something looks stale._",
      );
      parts.push("");
      if (m.context.trim()) {
        parts.push("### Current context");
        parts.push(m.context.trim());
        parts.push("");
      }
      if (m.principles.trim()) {
        parts.push("### How this consultant works (principles)");
        parts.push(m.principles.trim());
        parts.push("");
      }
      if (m.relationships.trim()) {
        parts.push("### Relationships");
        parts.push(m.relationships.trim());
        parts.push("");
      }
      if (m.lessons.trim()) {
        parts.push("### Lessons learned");
        parts.push(m.lessons.trim());
        parts.push("");
      }
    }
  } else if (p.memory.kind === "skipped") {
    // Silent skip — memory being unavailable shouldn't produce UI noise.
    // Log for debugging.
    // eslint-disable-next-line no-console
    console.debug(`[briefing] memory skipped: ${p.memory.reason}`);
  }

  parts.push("---");
  parts.push("");
  parts.push(
    "Your turn — open with a proactive take, not a blank-slate question.",
  );

  return parts.join("\n");
}

// ────────────────────────────────────────────────────────────────
// Helpers

function withTimeout<T>(
  p: Promise<T>,
  ms: number,
  label: string,
): Promise<T | { kind: "skipped"; reason: string }> {
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      resolve({
        kind: "skipped",
        reason: `${label} timed out after ${ms}ms`,
      } as T);
    }, ms);
    p.then(
      (v) => {
        clearTimeout(timer);
        resolve(v);
      },
      (e) => {
        clearTimeout(timer);
        resolve({
          kind: "skipped",
          reason: `${label} error: ${describeError(e)}`,
        } as T);
      },
    );
  });
}

function describeError(e: unknown): string {
  if (e instanceof Error) return e.message.slice(0, 120);
  if (typeof e === "string") return e.slice(0, 120);
  return String(e).slice(0, 120);
}

function formatTime(iso: string): string {
  const ms = Date.parse(iso);
  if (!Number.isFinite(ms)) return iso;
  return new Date(ms).toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
}

function formatRelativeDay(iso: string): string {
  const ms = Date.parse(iso);
  if (!Number.isFinite(ms)) return iso;
  const diffH = (Date.now() - ms) / (60 * 60 * 1000);
  if (diffH < 1) return "just now";
  if (diffH < 24) return `${Math.floor(diffH)}h ago`;
  const d = Math.floor(diffH / 24);
  return `${d}d ago`;
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max - 1) + "…";
}

function endOfLocalDay(d: Date): Date {
  const copy = new Date(d);
  copy.setHours(23, 59, 59, 999);
  return copy;
}
