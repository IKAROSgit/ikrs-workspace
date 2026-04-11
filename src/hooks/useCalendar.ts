import { useState, useEffect, useCallback } from "react";
import { useMcpStore } from "@/stores/mcpStore";
import { useEngagementStore } from "@/stores/engagementStore";

interface CalendarEvent {
  id: string;
  summary: string;
  start: string;
  end: string;
  location?: string;
  attendees: string[];
}

interface UseCalendarResult {
  events: CalendarEvent[];
  loading: boolean;
  error: string | null;
  isConnected: boolean;
  refresh: () => void;
}

export function useCalendar(): UseCalendarResult {
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const calServer = useMcpStore((s) =>
    s.servers.find((srv) => srv.type === "calendar"),
  );
  const isConnected = calServer?.status === "healthy";

  const refresh = useCallback(() => {
    if (!isConnected || !activeEngagementId) return;
    setLoading(true);
    setError(null);
    // MCP bridge pending — will fetch from Google Calendar via MCP server
    setEvents([]);
    setLoading(false);
  }, [isConnected, activeEngagementId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { events, loading, error, isConnected, refresh };
}
