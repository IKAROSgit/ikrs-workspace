import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useEngagementStore } from "@/stores/engagementStore";

export interface CalendarEvent {
  id: string;
  summary: string;
  start: string;
  end: string;
  location?: string | null;
  attendees: string[];
  hangout_link?: string | null;
  html_link?: string | null;
  status: string;
}

type CalendarResult =
  | { status: "ok"; events: CalendarEvent[] }
  | { status: "not_connected" }
  | { status: "scope_missing" }
  | { status: "rate_limited" }
  | { status: "network" }
  | { status: "other"; code: number | null };

export type CalendarConnectionState =
  | "loading"
  | "connected"
  | "not_connected"
  | "scope_missing"
  | "rate_limited"
  | "network"
  | "error";

interface UseCalendarResult {
  events: CalendarEvent[];
  loading: boolean;
  error: string | null;
  isConnected: boolean;
  connectionState: CalendarConnectionState;
  refresh: () => void;
}

const REFRESH_DEBOUNCE_MS = 1000;

/**
 * Google Calendar sync — direct REST v3. Same architecture as
 * useGmail: Rust command reads access token from keychain, hits the
 * REST API, returns a discriminated-union status so the UI can
 * branch on specific failure modes (not_connected / scope_missing /
 * rate_limited / network / other).
 *
 * Optimistic isConnected — stays true in "loading" and transient
 * error states so the view doesn't flash the Connect-Google empty
 * state on every engagement switch.
 */
export function useCalendar(): UseCalendarResult {
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [connectionState, setConnectionState] =
    useState<CalendarConnectionState>("loading");
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);

  const lastRefreshAt = useRef<number>(0);
  const inFlight = useRef<boolean>(false);
  const lastEngagement = useRef<string | null>(null);

  useEffect(() => {
    if (lastEngagement.current !== activeEngagementId) {
      lastRefreshAt.current = 0;
      lastEngagement.current = activeEngagementId ?? null;
    }
  }, [activeEngagementId]);

  const refresh = useCallback(async () => {
    if (!activeEngagementId) return;
    const callEngagement = activeEngagementId;
    const now = Date.now();
    if (inFlight.current) return;
    if (now - lastRefreshAt.current < REFRESH_DEBOUNCE_MS) return;
    lastRefreshAt.current = now;
    inFlight.current = true;
    setLoading(true);
    setError(null);

    try {
      const r = await invoke<CalendarResult>("list_calendar_events", {
        engagementId: callEngagement,
        daysAhead: 30,
        maxResults: 50,
      });
      // Rapid-engagement-switch guard (Codex must-fix 2026-04-20).
      const currentEngagement =
        useEngagementStore.getState().activeEngagementId;
      if (currentEngagement !== callEngagement) return;
      switch (r.status) {
        case "ok":
          setEvents(r.events);
          setConnectionState("connected");
          break;
        case "not_connected":
          setEvents([]);
          setConnectionState("not_connected");
          break;
        case "scope_missing":
          setConnectionState("scope_missing");
          setError(
            "Calendar permission not granted. Re-connect Google with Calendar access.",
          );
          break;
        case "rate_limited":
          setConnectionState("rate_limited");
          setError("Calendar rate limit reached. Retrying shortly.");
          break;
        case "network":
          setConnectionState("network");
          setError("Can't reach Google Calendar. Check your connection.");
          break;
        case "other":
          setConnectionState("error");
          setError(
            r.code
              ? `Calendar HTTP ${r.code}.`
              : "Calendar sync failed unexpectedly.",
          );
          break;
      }
    } catch (e) {
      const currentEngagement =
        useEngagementStore.getState().activeEngagementId;
      if (currentEngagement !== callEngagement) return;
      setError(e instanceof Error ? e.message : String(e));
      setConnectionState("error");
    } finally {
      setLoading(false);
      inFlight.current = false;
    }
  }, [activeEngagementId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const isConnected = connectionState !== "not_connected";

  return { events, loading, error, isConnected, connectionState, refresh };
}
