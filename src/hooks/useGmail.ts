import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useEngagementStore } from "@/stores/engagementStore";

interface Email {
  id: string;
  from: string;
  subject: string;
  snippet: string;
  date: string;
  isRead: boolean;
}

interface GmailMessageFromRust {
  id: string;
  thread_id: string;
  from: string;
  subject: string;
  snippet: string;
  date: string;
  is_read: boolean;
}

type GmailInboxResult =
  | { status: "ok"; messages: GmailMessageFromRust[] }
  | { status: "not_connected" }
  | { status: "scope_missing" }
  | { status: "rate_limited" }
  | { status: "network" }
  | { status: "other"; code: number | null };

export type GmailConnectionState =
  | "loading"
  | "connected"
  | "not_connected"
  | "scope_missing"
  | "rate_limited"
  | "network"
  | "error";

interface UseGmailResult {
  emails: Email[];
  loading: boolean;
  error: string | null;
  isConnected: boolean;
  connectionState: GmailConnectionState;
  refresh: () => void;
}

const REFRESH_DEBOUNCE_MS = 1000;

/**
 * Gmail inbox sync — direct Gmail REST from Rust.
 *
 * Rapid-engagement-switch safety (Codex 2026-04-20 must-fix): every
 * async refresh captures the engagementId at call start. When the
 * response lands, we discard it if the active engagement has
 * changed in the meantime — otherwise engagement A's emails would
 * momentarily overwrite the engagement-B view the user switched
 * to. The debounce also resets on engagement change so switching
 * doesn't eat the new engagement's legitimate first refresh.
 */
export function useGmail(): UseGmailResult {
  const [emails, setEmails] = useState<Email[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [connectionState, setConnectionState] =
    useState<GmailConnectionState>("loading");
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);

  const lastRefreshAt = useRef<number>(0);
  const inFlight = useRef<boolean>(false);
  const lastEngagement = useRef<string | null>(null);

  // Reset debounce when the engagement changes.
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
      const result = await invoke<GmailInboxResult>("list_gmail_inbox", {
        engagementId: callEngagement,
        maxResults: 30,
      });
      // Discard the result if the user switched engagements
      // between call start and response arrival.
      const currentEngagement =
        useEngagementStore.getState().activeEngagementId;
      if (currentEngagement !== callEngagement) {
        return;
      }

      switch (result.status) {
        case "ok": {
          const mapped: Email[] = result.messages.map((m) => ({
            id: m.id,
            from: m.from || "(unknown sender)",
            subject: m.subject || "(no subject)",
            snippet: m.snippet,
            date: formatDate(m.date),
            isRead: m.is_read,
          }));
          setEmails(mapped);
          setConnectionState("connected");
          break;
        }
        case "not_connected":
          setEmails([]);
          setConnectionState("not_connected");
          break;
        case "scope_missing":
          setEmails([]);
          setConnectionState("scope_missing");
          setError(
            "Gmail read permission not granted. Re-connect Google in Settings and grant Gmail access.",
          );
          break;
        case "rate_limited":
          setConnectionState("rate_limited");
          setError(
            "Gmail rate limit reached. Waiting a bit before the next sync.",
          );
          break;
        case "network":
          setConnectionState("network");
          setError("Can't reach Gmail. Check your internet connection.");
          break;
        case "other":
          setConnectionState("error");
          setError(
            result.code
              ? `Gmail returned HTTP ${result.code}. Try again in a moment.`
              : "Gmail sync failed with an unexpected error.",
          );
          break;
      }
    } catch (e) {
      const currentEngagement =
        useEngagementStore.getState().activeEngagementId;
      if (currentEngagement !== callEngagement) return;
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
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

  return { emails, loading, error, isConnected, connectionState, refresh };
}

/** RFC 2822 → compact display. */
function formatDate(raw: string): string {
  if (!raw) return "";
  const d = new Date(raw);
  if (Number.isNaN(d.getTime())) return raw;
  const now = new Date();
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  if (sameDay) {
    return d.toLocaleTimeString(undefined, {
      hour: "numeric",
      minute: "2-digit",
    });
  }
  const sameYear = d.getFullYear() === now.getFullYear();
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: sameYear ? undefined : "numeric",
  });
}
