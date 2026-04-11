import { useState, useEffect, useCallback } from "react";
import { useMcpStore } from "@/stores/mcpStore";
import { useEngagementStore } from "@/stores/engagementStore";

interface Email {
  id: string;
  from: string;
  subject: string;
  snippet: string;
  date: string;
  isRead: boolean;
}

interface UseGmailResult {
  emails: Email[];
  loading: boolean;
  error: string | null;
  isConnected: boolean;
  refresh: () => void;
}

export function useGmail(): UseGmailResult {
  const [emails, setEmails] = useState<Email[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const gmailServer = useMcpStore((s) =>
    s.servers.find((srv) => srv.type === "gmail"),
  );
  const isConnected = gmailServer?.status === "healthy";

  const refresh = useCallback(() => {
    if (!isConnected || !activeEngagementId) return;
    setLoading(true);
    setError(null);
    // MCP communication will be wired when MCP client bridge is implemented.
    setEmails([]);
    setLoading(false);
  }, [isConnected, activeEngagementId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { emails, loading, error, isConnected, refresh };
}
