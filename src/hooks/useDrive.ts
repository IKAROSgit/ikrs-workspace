import { useState, useEffect, useCallback } from "react";
import { useMcpStore } from "@/stores/mcpStore";
import { useEngagementStore } from "@/stores/engagementStore";

interface DriveFile {
  id: string;
  name: string;
  mimeType: string;
  modifiedTime: string;
  size?: string;
  webViewLink?: string;
}

interface UseDriveResult {
  files: DriveFile[];
  loading: boolean;
  error: string | null;
  isConnected: boolean;
  refresh: () => void;
  search: (query: string) => void;
}

export function useDrive(): UseDriveResult {
  const [files, setFiles] = useState<DriveFile[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const driveServer = useMcpStore((s) =>
    s.servers.find((srv) => srv.type === "drive"),
  );
  const isConnected = driveServer?.status === "healthy";

  const refresh = useCallback(() => {
    if (!isConnected || !activeEngagementId) return;
    setLoading(true);
    setError(null);
    // MCP communication will be wired when MCP client bridge is implemented.
    setFiles([]);
    setLoading(false);
  }, [isConnected, activeEngagementId]);

  const search = useCallback(
    (_query: string) => {
      if (!isConnected || !activeEngagementId) return;
      setLoading(true);
      setError(null);
      // MCP bridge pending — will query Drive via MCP tool call.
      setFiles([]);
      setLoading(false);
    },
    [isConnected, activeEngagementId],
  );

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { files, loading, error, isConnected, refresh, search };
}
