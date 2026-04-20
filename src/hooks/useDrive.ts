import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useEngagementStore } from "@/stores/engagementStore";

export interface DriveFile {
  id: string;
  name: string;
  mimeType: string;
  modifiedTime: string;
  size?: string | null;
  webViewLink?: string | null;
}

type DriveResult =
  | { status: "ok"; files: DriveFile[] }
  | { status: "not_connected" }
  | { status: "scope_missing" }
  | { status: "rate_limited" }
  | { status: "network" }
  | { status: "other"; code: number | null };

export type DriveConnectionState =
  | "loading"
  | "connected"
  | "not_connected"
  | "scope_missing"
  | "rate_limited"
  | "network"
  | "error";

interface UseDriveResult {
  files: DriveFile[];
  loading: boolean;
  error: string | null;
  isConnected: boolean;
  connectionState: DriveConnectionState;
  refresh: () => void;
  search: (query: string) => void;
}

const REFRESH_DEBOUNCE_MS = 1000;

/**
 * Google Drive sync — direct REST v3. Same pattern as useGmail /
 * useCalendar. `search(query)` issues the Drive query syntax
 * `name contains 'query' and trashed=false`; empty/no query falls
 * back to the user's own recent files.
 */
export function useDrive(): UseDriveResult {
  const [files, setFiles] = useState<DriveFile[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [connectionState, setConnectionState] =
    useState<DriveConnectionState>("loading");
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);

  const lastRefreshAt = useRef<number>(0);
  const inFlight = useRef<boolean>(false);
  const currentQuery = useRef<string | null>(null);
  const lastEngagement = useRef<string | null>(null);

  useEffect(() => {
    if (lastEngagement.current !== activeEngagementId) {
      lastRefreshAt.current = 0;
      lastEngagement.current = activeEngagementId ?? null;
    }
  }, [activeEngagementId]);

  const runList = useCallback(
    async (query: string | null) => {
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
        const r = await invoke<DriveResult>("list_drive_files", {
          engagementId: callEngagement,
          query: query ?? null,
          maxResults: 50,
        });
        const currentEngagement =
          useEngagementStore.getState().activeEngagementId;
        if (currentEngagement !== callEngagement) return;
        switch (r.status) {
          case "ok":
            setFiles(r.files);
            setConnectionState("connected");
            break;
          case "not_connected":
            setFiles([]);
            setConnectionState("not_connected");
            break;
          case "scope_missing":
            setConnectionState("scope_missing");
            setError(
              "Drive permission not granted. Re-connect Google with Drive access.",
            );
            break;
          case "rate_limited":
            setConnectionState("rate_limited");
            setError("Drive rate limit reached.");
            break;
          case "network":
            setConnectionState("network");
            setError("Can't reach Google Drive. Check your connection.");
            break;
          case "other":
            setConnectionState("error");
            setError(
              r.code
                ? `Drive HTTP ${r.code}.`
                : "Drive sync failed unexpectedly.",
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
    },
    [activeEngagementId],
  );

  const refresh = useCallback(() => {
    void runList(currentQuery.current);
  }, [runList]);

  const search = useCallback(
    (query: string) => {
      const q = query.trim();
      currentQuery.current = q.length > 0 ? q : null;
      // Bypass the debounce for explicit user-typed searches — they
      // naturally gap on keystroke cadence but we don't want to drop
      // a real search because it landed within 1s of a refresh.
      lastRefreshAt.current = 0;
      void runList(currentQuery.current);
    },
    [runList],
  );

  useEffect(() => {
    void runList(null);
  }, [runList]);

  const isConnected = connectionState !== "not_connected";

  return {
    files,
    loading,
    error,
    isConnected,
    connectionState,
    refresh,
    search,
  };
}
