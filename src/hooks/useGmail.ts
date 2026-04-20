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

// Discriminated union matching the Rust `GmailInboxResult` enum
// (commands::gmail_sync). Tauri serde v1 serializes with the
// `tag = "status"` attribute, variant name is snake_case of the
// Rust variant. See gmail_sync.rs module docstring for why this
// shape rather than stringly-matched Err.
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
  /** Legacy boolean kept so existing callers don't break. `true`
   *  when connectionState is "connected" or still loading the
   *  first refresh (optimistic — prevents the Connect-Google flash
   *  on mount). See Codex must-fix #1 (2026-04-20). */
  isConnected: boolean;
  connectionState: GmailConnectionState;
  refresh: () => void;
}

const REFRESH_DEBOUNCE_MS = 1000;

/**
 * Gmail inbox sync.
 *
 * 2026-04-20 rewrite: previously this hook was a stub returning
 * `[]` — Inbox view showed "No emails" forever. Now it calls the
 * Rust `list_gmail_inbox` command, which hits Gmail REST directly
 * using the per-engagement access token from keychain.
 *
 * Codex-addressed must-fixes this revision:
 *  #1 optimistic isConnected: starts in "loading" so InboxView
 *     doesn't flash "Connect Google in Settings" on every mount
 *     while the first fetch is in flight.
 *  #2 structured error taxonomy: branch on discriminated union
 *     from Rust, not substring match on error messages.
 *  #4 refresh debounce: rapid clicks collapse into one call.
 *  #5 NotConnected vs empty-inbox: Rust returns distinct variants,
 *     and the hook shows the Connect-Google state only for
 *     NotConnected, never for a legitimately empty inbox.
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

  const refresh = useCallback(async () => {
    if (!activeEngagementId) return;
    const now = Date.now();
    if (inFlight.current) return;
    if (now - lastRefreshAt.current < REFRESH_DEBOUNCE_MS) return;
    lastRefreshAt.current = now;
    inFlight.current = true;
    setLoading(true);
    setError(null);

    try {
      const result = await invoke<GmailInboxResult>("list_gmail_inbox", {
        engagementId: activeEngagementId,
        maxResults: 30,
      });

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
      // Transport / serialization failure from the Tauri invoke
      // itself. Not Gmail-specific.
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

  // Legacy boolean kept for InboxView's existing branch:
  //   false → show Connect-Google empty state
  //   true  → render the inbox UI (possibly with a red error banner
  //           if `error` is set from a transient rate_limit/network)
  // Only `not_connected` means "please connect Google"; everything
  // else (including "loading" and transient errors) should keep the
  // inbox chrome visible so the user doesn't lose orientation.
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
