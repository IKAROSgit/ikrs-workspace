import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { RefreshCw, Calendar, CalendarPlus } from "lucide-react";
import { useCalendar } from "@/hooks/useCalendar";
import { useEngagementStore } from "@/stores/engagementStore";
import { OfflineBanner } from "@/components/OfflineBanner";

export default function CalendarView() {
  const { events, loading, error, isConnected, refresh } = useCalendar();
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);

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
          <Button variant="ghost" size="sm">
            <CalendarPlus size={14} />
          </Button>
        </div>
      </div>

      {error && (
        <div className="px-4 py-2 bg-destructive/10 text-destructive text-sm">
          {error}
        </div>
      )}

      <ScrollArea className="flex-1">
        {events.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
            <p className="text-sm">No upcoming events.</p>
          </div>
        ) : (
          <div className="divide-y divide-border">
            {events.map((event) => (
              <div
                key={event.id}
                className="flex flex-col gap-1 px-4 py-3 hover:bg-accent/50"
              >
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium">{event.summary}</span>
                  <span className="text-xs text-muted-foreground">
                    {event.start}
                  </span>
                </div>
                {event.location && (
                  <span className="text-xs text-muted-foreground">
                    {event.location}
                  </span>
                )}
              </div>
            ))}
          </div>
        )}
      </ScrollArea>
    </div>
  );
}
