import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useEngagementStore } from "@/stores/engagementStore";

export interface VaultFile {
  path: string;
  name: string;
  rel_path: string;
  is_directory: boolean;
  size_bytes: number;
  modified_unix: number;
}

type NotesResult =
  | { status: "ok"; files: VaultFile[] }
  | { status: "no_vault" }
  | { status: "other"; message: string };

export type NotesConnectionState =
  | "loading"
  | "connected"
  | "no_vault"
  | "error";

interface UseNotesResult {
  files: VaultFile[];
  loading: boolean;
  error: string | null;
  isConnected: boolean;
  connectionState: NotesConnectionState;
  refresh: () => void;
  readContent: (relPath: string) => Promise<string>;
}

const REFRESH_DEBOUNCE_MS = 500;

/**
 * Local notes — reads the engagement vault filesystem directly.
 * No network, no MCP. Vault lives at
 * `~/.ikrs-workspace/vaults/<slug>/`.
 *
 * Rebuilt 2026-04-20 from the MCP-stubbed placeholder. Refresh is
 * instant (single directory walk in Rust, symlinks not followed,
 * depth capped at 5). Content for a specific file is fetched lazily
 * via `readContent(rel_path)` so the list doesn't memory-load the
 * entire vault.
 */
export function useNotes(): UseNotesResult {
  const [files, setFiles] = useState<VaultFile[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [connectionState, setConnectionState] =
    useState<NotesConnectionState>("loading");
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const engagements = useEngagementStore((s) => s.engagements);
  const clients = useEngagementStore((s) => s.clients);

  const clientSlug = (() => {
    const eng = engagements.find((e) => e.id === activeEngagementId);
    if (!eng) return null;
    const client = clients.find((c) => c.id === eng.clientId);
    return client?.slug ?? null;
  })();

  const lastRefreshAt = useRef<number>(0);
  const inFlight = useRef<boolean>(false);
  const lastSlug = useRef<string | null>(null);

  useEffect(() => {
    if (lastSlug.current !== clientSlug) {
      lastRefreshAt.current = 0;
      lastSlug.current = clientSlug;
    }
  }, [clientSlug]);

  const refresh = useCallback(async () => {
    if (!clientSlug) return;
    const callSlug = clientSlug;
    const now = Date.now();
    if (inFlight.current) return;
    if (now - lastRefreshAt.current < REFRESH_DEBOUNCE_MS) return;
    lastRefreshAt.current = now;
    inFlight.current = true;
    setLoading(true);
    setError(null);

    try {
      const r = await invoke<NotesResult>("list_vault_notes", {
        clientSlug: callSlug,
      });
      // If the user switched engagements while the FS walk was in
      // flight, discard — the next engagement's own refresh will
      // run. This is lightweight here (local walk) but matches the
      // pattern used for the Google hooks for consistency.
      if (lastSlug.current !== callSlug) return;
      switch (r.status) {
        case "ok":
          setFiles(r.files);
          setConnectionState("connected");
          break;
        case "no_vault":
          setFiles([]);
          setConnectionState("no_vault");
          break;
        case "other":
          setConnectionState("error");
          setError(r.message);
          break;
      }
    } catch (e) {
      if (lastSlug.current !== callSlug) return;
      setError(e instanceof Error ? e.message : String(e));
      setConnectionState("error");
    } finally {
      setLoading(false);
      inFlight.current = false;
    }
  }, [clientSlug]);

  const readContent = useCallback(
    async (relPath: string) => {
      if (!clientSlug) throw new Error("No active engagement");
      return invoke<string>("read_note_content", {
        clientSlug,
        relPath,
      });
    },
    [clientSlug],
  );

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const isConnected = connectionState === "connected";

  return {
    files,
    loading,
    error,
    isConnected,
    connectionState,
    refresh,
    readContent,
  };
}
