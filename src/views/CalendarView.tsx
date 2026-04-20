import { useState } from "react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { RefreshCw, Calendar, CalendarPlus } from "lucide-react";
import { useCalendar, type CalendarEvent } from "@/hooks/useCalendar";
import { useEngagementStore } from "@/stores/engagementStore";
import { OfflineBanner } from "@/components/OfflineBanner";
import { MeetingPrepPanel } from "@/components/calendar/MeetingPrepPanel";
import { QuickAddEventModal } from "@/components/calendar/QuickAddEventModal";

export default function CalendarView() {
  const { events, loading, error, isConnected, refresh } = useCalendar();
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const [openEvent, setOpenEvent] = useState<CalendarEvent | null>(null);
  const [quickAddOpen, setQuickAddOpen] = useState(false);

  if (!activeEngagementId) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
        <Calendar size={48} className="mb-4 opacity-50" />
        <p>Select an engagement to view calendar.</p>
      </div>
    );
  }

  if (!isConnected) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
        <Calendar size={48} className="mb-4 opacity-50" />
        <p>Connect a Google account in Settings to view events.</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <OfflineBanner feature="Google Calendar" />
      <div className="flex items-center justify-between px-4 py-2 border-b border-border">
        <h2 className="text-sm font-semibold">Calendar</h2>
        <div className="flex gap-1">
          <Button variant="ghost" size="sm" onClick={refresh} disabled={loading}>
            <RefreshCw size={14} className={loading ? "animate-spin" : ""} />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setQuickAddOpen(true)}
            title="New event"
          >
            <CalendarPlus size={14} />
          </Button>
        </div>
      </div>

      {error && (
        <div className="px-4 py-2 bg-destructive/10 text-destructive text-sm">
          {error}
        </div>
      )}

      <div className="flex-1 flex overflow-hidden">
        <ScrollArea className="flex-1">
          {events.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
              <p className="text-sm">
                {loading ? "Loading events..." : "No upcoming events."}
              </p>
            </div>
          ) : (
            <div className="divide-y divide-border">
              {events.map((event) => (
                <button
                  type="button"
                  key={event.id}
                  onClick={() => setOpenEvent(event)}
                  className={`w-full text-left flex flex-col gap-1 px-4 py-3 hover:bg-accent/50 cursor-pointer ${
                    openEvent?.id === event.id ? "bg-accent/70" : ""
                  }`}
                >
                  <div className="flex items-center justify-between gap-3">
                    <span className="text-sm font-medium flex-1 truncate">
                      {event.summary || "(untitled)"}
                    </span>
                    <span className="text-xs text-muted-foreground whitespace-nowrap">
                      {formatEventTime(event.start, event.end)}
                    </span>
                  </div>
                  {event.location && (
                    <span className="text-xs text-muted-foreground truncate">
                      📍 {event.location}
                    </span>
                  )}
                  {event.attendees.length > 0 && (
                    <span className="text-xs text-muted-foreground truncate">
                      👥 {event.attendees.slice(0, 3).join(", ")}
                      {event.attendees.length > 3 &&
                        ` +${event.attendees.length - 3}`}
                    </span>
                  )}
                  {event.hangout_link && (
                    <span className="text-xs text-blue-500 truncate">
                      🎥 Meet link
                    </span>
                  )}
                </button>
              ))}
            </div>
          )}
        </ScrollArea>
        {openEvent && (
          <MeetingPrepPanel
            event={openEvent}
            onClose={() => setOpenEvent(null)}
          />
        )}
      </div>
      {quickAddOpen && (
        <QuickAddEventModal
          onClose={() => {
            setQuickAddOpen(false);
            refresh();
          }}
        />
      )}
    </div>
  );
}

/** Format a calendar event's start/end for compact display.
 *  - Today  → "10:00 – 11:00"
 *  - Tomorrow / soon → "Tue 10:00"
 *  - Later  → "Apr 25"
 *  Accepts either RFC 3339 dateTime or YYYY-MM-DD all-day strings. */
function formatEventTime(start: string, end: string): string {
  if (!start) return "";
  const s = new Date(start);
  if (Number.isNaN(s.getTime())) return start;
  const now = new Date();
  const sameDay =
    s.getFullYear() === now.getFullYear() &&
    s.getMonth() === now.getMonth() &&
    s.getDate() === now.getDate();

  const isAllDay = !start.includes("T"); // YYYY-MM-DD
  if (isAllDay) {
    return s.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
    });
  }

  const timeStr = s.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
  if (sameDay) {
    if (end) {
      const e = new Date(end);
      if (!Number.isNaN(e.getTime())) {
        const endStr = e.toLocaleTimeString(undefined, {
          hour: "numeric",
          minute: "2-digit",
        });
        return `${timeStr} – ${endStr}`;
      }
    }
    return timeStr;
  }
  const dayStr = s.toLocaleDateString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
  });
  return `${dayStr} ${timeStr}`;
}
