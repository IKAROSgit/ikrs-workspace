import { useState } from "react";
import { X, CalendarPlus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { createCalendarEvent } from "@/lib/tauri-commands";
import { useEngagementStore } from "@/stores/engagementStore";

/**
 * Quick-add Calendar event. Minimal 4-field form — summary, date,
 * start time, duration — plus optional location + attendees.
 * Assumes local timezone; the Rust side just passes the RFC 3339
 * string straight through to Google Calendar which interprets it.
 *
 * `sendUpdates=all` is set server-side so external attendees get
 * an invitation email.
 */
export function QuickAddEventModal({ onClose }: { onClose: () => void }) {
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const [summary, setSummary] = useState("");
  const [date, setDate] = useState(() => new Date().toISOString().slice(0, 10));
  const [startTime, setStartTime] = useState("10:00");
  const [duration, setDuration] = useState("30");
  const [location, setLocation] = useState("");
  const [attendees, setAttendees] = useState("");
  const [description, setDescription] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  const handleCreate = async () => {
    if (!activeEngagementId) {
      setError("No active engagement.");
      return;
    }
    if (!summary.trim()) {
      setError("Title required.");
      return;
    }
    const start = new Date(`${date}T${startTime}`);
    if (Number.isNaN(start.getTime())) {
      setError("Invalid date/time.");
      return;
    }
    const durationMin = parseInt(duration, 10) || 30;
    const end = new Date(start.getTime() + durationMin * 60_000);
    const startIso = toLocalIsoWithOffset(start);
    const endIso = toLocalIsoWithOffset(end);
    const atts = attendees
      .split(/[,;\n]/)
      .map((a) => a.trim())
      .filter((a) => a.length > 0);

    setSubmitting(true);
    setError(null);
    try {
      const r = await createCalendarEvent({
        engagementId: activeEngagementId,
        summary: summary.trim(),
        startIso,
        endIso,
        location: location.trim() || null,
        description: description.trim() || null,
        attendees: atts,
      });
      switch (r.status) {
        case "ok":
          setDone(r.html_link);
          setTimeout(onClose, 900);
          break;
        case "not_connected":
          setError("Google not connected. Reconnect in Settings.");
          break;
        case "scope_missing":
          setError("Calendar write permission missing.");
          break;
        case "rate_limited":
          setError("Rate limit. Try again in a minute.");
          break;
        case "network":
          setError("Network issue. Check your connection.");
          break;
        case "invalid":
          setError(r.message);
          break;
        case "other":
          setError(
            r.code
              ? `Calendar returned HTTP ${r.code}.`
              : "Unknown error creating event.",
          );
          break;
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  };

  const hasContent =
    summary.trim() !== "" ||
    location.trim() !== "" ||
    attendees.trim() !== "" ||
    description.trim() !== "";

  const confirmClose = () => {
    if (done || !hasContent) {
      onClose();
      return;
    }
    if (window.confirm("Discard this event? Your input will be lost.")) {
      onClose();
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-background/70 backdrop-blur-sm"
    >
      <div
        className="w-full max-w-lg rounded-lg border border-border bg-popover shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-4 py-2 border-b border-border">
          <h3 className="text-sm font-semibold">New event</h3>
          <Button variant="ghost" size="sm" onClick={confirmClose}>
            <X size={14} />
          </Button>
        </div>
        <div className="p-4 space-y-3">
          <Input
            value={summary}
            onChange={(e) => setSummary(e.target.value)}
            placeholder="Title"
            className="h-9"
            disabled={submitting || !!done}
            autoFocus
          />
          <div className="grid grid-cols-3 gap-2">
            <Input
              type="date"
              value={date}
              onChange={(e) => setDate(e.target.value)}
              className="h-8 text-sm"
              disabled={submitting || !!done}
            />
            <Input
              type="time"
              value={startTime}
              onChange={(e) => setStartTime(e.target.value)}
              className="h-8 text-sm"
              disabled={submitting || !!done}
            />
            <div className="flex items-center gap-1">
              <Input
                type="number"
                min="5"
                max="480"
                value={duration}
                onChange={(e) => setDuration(e.target.value)}
                className="h-8 text-sm"
                disabled={submitting || !!done}
              />
              <span className="text-xs text-muted-foreground">min</span>
            </div>
          </div>
          <Input
            value={location}
            onChange={(e) => setLocation(e.target.value)}
            placeholder="Location (optional)"
            className="h-8 text-sm"
            disabled={submitting || !!done}
          />
          <Input
            value={attendees}
            onChange={(e) => setAttendees(e.target.value)}
            placeholder="Attendees — comma-separated emails"
            className="h-8 text-sm"
            disabled={submitting || !!done}
          />
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Description (optional)"
            disabled={submitting || !!done}
            className="w-full h-24 text-sm p-2 bg-background border border-border rounded-md resize-none focus:outline-none focus:ring-2 focus:ring-primary"
          />
          {error && (
            <div className="text-sm text-destructive bg-destructive/10 p-2 rounded">
              {error}
            </div>
          )}
          {done && (
            <div className="text-sm text-green-600 dark:text-green-400 bg-green-500/10 p-2 rounded">
              Event created.{" "}
              {done && (
                <a
                  href={done}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="underline"
                >
                  Open in Calendar
                </a>
              )}
            </div>
          )}
        </div>
        <div className="flex items-center justify-end gap-2 px-4 py-2 border-t border-border">
          <Button variant="ghost" size="sm" onClick={confirmClose} disabled={submitting}>
            Cancel
          </Button>
          <Button size="sm" onClick={handleCreate} disabled={submitting || !!done}>
            <CalendarPlus size={14} className="mr-1.5" />
            {submitting ? "Creating…" : done ? "Created" : "Create event"}
          </Button>
        </div>
      </div>
    </div>
  );
}

/** Format a local Date as RFC 3339 with the current timezone offset.
 *  e.g. 2026-04-25T10:00:00+04:00 — Google Calendar accepts this. */
function toLocalIsoWithOffset(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  const offMin = -d.getTimezoneOffset();
  const sign = offMin >= 0 ? "+" : "-";
  const offH = pad(Math.floor(Math.abs(offMin) / 60));
  const offM = pad(Math.abs(offMin) % 60);
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` +
    `T${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}` +
    `${sign}${offH}:${offM}`
  );
}
