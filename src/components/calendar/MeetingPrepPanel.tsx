import { useMemo } from "react";
import { X, Mail, FolderOpen, FileText, Video, MapPin } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useGmail } from "@/hooks/useGmail";
import { useDrive } from "@/hooks/useDrive";
import { useNotes } from "@/hooks/useNotes";
import type { CalendarEvent } from "@/hooks/useCalendar";

/**
 * Meeting prep brief — appears when a consultant clicks a calendar
 * event. Pulls together in one place:
 *   - The event metadata (time, location, attendees, Meet link)
 *   - Recent emails FROM or TO any attendee
 *   - Drive files whose name contains any token from the event
 *     summary (fuzzy)
 *   - Vault notes with matching name / path
 *
 * All data is read client-side from already-loaded hook caches —
 * no new API calls, no backend commit. Sub-100ms render.
 *
 * Design principles:
 *   - Be boring. The goal is "you arrive at the meeting prepared",
 *     not "you get dazzled by AI". Plain lists, tight information
 *     density, every item has a link/action to go deeper.
 *   - Attendee matching is email-domain-flexible: "moe@ikaros.ae"
 *     in the event matches a From header of
 *     "Moe Aqeel <moe@ikaros.ae>". Fallback to display-name
 *     substring if no email match.
 */
export function MeetingPrepPanel({
  event,
  onClose,
}: {
  event: CalendarEvent;
  onClose: () => void;
}) {
  const { emails } = useGmail();
  const { files: driveFiles } = useDrive();
  const { files: vaultFiles } = useNotes();

  const tokens = useMemo(() => summaryTokens(event.summary), [event.summary]);
  const attendeeNeedles = useMemo(
    () => event.attendees.map((a) => a.toLowerCase()),
    [event.attendees],
  );

  const relatedEmails = useMemo(() => {
    if (attendeeNeedles.length === 0) return [];
    return emails
      .filter((e) => {
        const hay = (e.from + " " + e.subject).toLowerCase();
        return attendeeNeedles.some((n) => n && hay.includes(n));
      })
      .slice(0, 6);
  }, [emails, attendeeNeedles]);

  const relatedDrive = useMemo(() => {
    if (tokens.length === 0) return [];
    return driveFiles
      .filter((f) => {
        const name = f.name.toLowerCase();
        return tokens.some((t) => t && name.includes(t));
      })
      .slice(0, 6);
  }, [driveFiles, tokens]);

  const relatedNotes = useMemo(() => {
    if (tokens.length === 0) return [];
    return vaultFiles
      .filter((f) => {
        const name = (f.name + " " + f.rel_path).toLowerCase();
        return tokens.some((t) => t && name.includes(t));
      })
      .slice(0, 6);
  }, [vaultFiles, tokens]);

  return (
    <div className="border-l border-border bg-background w-96 flex-shrink-0 flex flex-col overflow-hidden">
      <div className="flex items-start gap-2 px-4 py-3 border-b border-border">
        <div className="flex-1 min-w-0">
          <div className="text-sm font-semibold truncate">
            {event.summary || "(untitled)"}
          </div>
          <div className="text-xs text-muted-foreground">
            {formatRange(event.start, event.end)}
          </div>
        </div>
        <Button variant="ghost" size="sm" onClick={onClose}>
          <X size={14} />
        </Button>
      </div>
      <ScrollArea className="flex-1">
        <div className="p-4 space-y-5 text-sm">
          {/* Event core */}
          <section className="space-y-1.5">
            {event.location && (
              <div className="flex items-start gap-2 text-muted-foreground">
                <MapPin size={14} className="mt-0.5 flex-shrink-0" />
                <span>{event.location}</span>
              </div>
            )}
            {event.hangout_link && (
              <a
                href={event.hangout_link}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center gap-2 text-blue-500 hover:underline"
              >
                <Video size={14} /> Join Google Meet
              </a>
            )}
            {event.attendees.length > 0 && (
              <div>
                <div className="text-[10px] uppercase tracking-wider text-muted-foreground mb-1">
                  Attendees ({event.attendees.length})
                </div>
                <div className="flex flex-wrap gap-1">
                  {event.attendees.map((a, i) => (
                    <span
                      key={`${a}-${i}`}
                      className="text-xs px-1.5 py-0.5 bg-muted rounded"
                    >
                      {a}
                    </span>
                  ))}
                </div>
              </div>
            )}
          </section>

          {/* Recent emails from attendees */}
          <section>
            <div className="flex items-center gap-2 text-[10px] uppercase tracking-wider text-muted-foreground mb-2">
              <Mail size={12} /> Recent emails with attendees
            </div>
            {relatedEmails.length === 0 ? (
              <div className="text-xs text-muted-foreground">
                No recent emails matching attendee names.
              </div>
            ) : (
              <ul className="space-y-1.5">
                {relatedEmails.map((m) => (
                  <li
                    key={m.id}
                    className="text-xs border-l-2 border-border pl-2"
                    title={`${m.from}\n${m.snippet}`}
                  >
                    <div className="font-medium truncate">{m.subject}</div>
                    <div className="text-muted-foreground truncate">
                      {m.from} · {m.date}
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </section>

          {/* Drive files matching event title */}
          <section>
            <div className="flex items-center gap-2 text-[10px] uppercase tracking-wider text-muted-foreground mb-2">
              <FolderOpen size={12} /> Related Drive files
            </div>
            {relatedDrive.length === 0 ? (
              <div className="text-xs text-muted-foreground">
                No Drive files match the event title.
              </div>
            ) : (
              <ul className="space-y-1.5">
                {relatedDrive.map((f) => (
                  <li key={f.id} className="text-xs">
                    {f.webViewLink ? (
                      <a
                        href={f.webViewLink}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="block truncate hover:underline"
                      >
                        {f.name}
                      </a>
                    ) : (
                      <span className="block truncate">{f.name}</span>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </section>

          {/* Vault notes matching event title */}
          <section>
            <div className="flex items-center gap-2 text-[10px] uppercase tracking-wider text-muted-foreground mb-2">
              <FileText size={12} /> Related notes
            </div>
            {relatedNotes.length === 0 ? (
              <div className="text-xs text-muted-foreground">
                No notes match the event title.
              </div>
            ) : (
              <ul className="space-y-1.5">
                {relatedNotes.map((n) => (
                  <li
                    key={n.path}
                    className="text-xs truncate"
                    title={n.rel_path}
                  >
                    {n.name}
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>
      </ScrollArea>
    </div>
  );
}

/** Break an event summary into lowercase alphanumeric tokens >= 3
 *  characters. Removes common filler words. */
function summaryTokens(summary: string): string[] {
  if (!summary) return [];
  const stop = new Set([
    "the",
    "and",
    "with",
    "for",
    "meeting",
    "call",
    "sync",
    "catch",
    "up",
    "review",
    "session",
  ]);
  return summary
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((t) => t.length >= 3 && !stop.has(t));
}

function formatRange(start: string, end: string): string {
  if (!start) return "";
  const s = new Date(start);
  if (Number.isNaN(s.getTime())) return start;
  const sTime = s.toLocaleString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
  if (!end) return sTime;
  const e = new Date(end);
  if (Number.isNaN(e.getTime())) return sTime;
  const eTime = e.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
  return `${sTime} – ${eTime}`;
}
