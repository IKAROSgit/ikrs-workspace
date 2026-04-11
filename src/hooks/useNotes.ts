import { useState, useCallback } from "react";
import { useMcpStore } from "@/stores/mcpStore";
import { useEngagementStore } from "@/stores/engagementStore";

interface VaultFile {
  path: string;
  name: string;
  content?: string;
  isDirectory: boolean;
}

interface UseNotesResult {
  files: VaultFile[];
  loading: boolean;
  error: string | null;
  isConnected: boolean;
  refresh: () => void;
}

export function useNotes(): UseNotesResult {
  const [files, setFiles] = useState<VaultFile[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const obsidianServer = useMcpStore((s) =>
    s.servers.find((srv) => srv.type === "obsidian"),
  );
  const isConnected = obsidianServer?.status === "healthy";

  const refresh = useCallback(() => {
    if (!isConnected || !activeEngagementId) return;
    setLoading(true);
    setError(null);
    setFiles([]);
    setLoading(false);
  }, [isConnected, activeEngagementId]);

  return { files, loading, error, isConnected, refresh };
}
