import { useEffect, useRef } from "react";
import { useClaudeStore } from "@/stores/claudeStore";
import { useEngagementStore } from "@/stores/engagementStore";

interface SessionDetailsModalProps {
  onClose: () => void;
}

function formatDuration(startMs: number | null): string {
  if (!startMs) return "—";
  const seconds = Math.floor((Date.now() - startMs) / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}m ${remainingSeconds}s`;
}

export function SessionDetailsModal({ onClose }: SessionDetailsModalProps) {
  const sessionId = useClaudeStore((s) => s.sessionId);
  const status = useClaudeStore((s) => s.status);
  const model = useClaudeStore((s) => s.model);
  const totalCostUsd = useClaudeStore((s) => s.totalCostUsd);
  const sessionStartedAt = useClaudeStore((s) => s.sessionStartedAt);

  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const engagements = useEngagementStore((s) => s.engagements);
  const clients = useEngagementStore((s) => s.clients);

  const engagement = engagements.find((e) => e.id === activeEngagementId);
  const client = clients.find((c) => c.id === engagement?.clientId);

  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        onClose();
      }
    };
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleEscape);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleEscape);
    };
  }, [onClose]);

  const STATUS_LABELS: Record<string, string> = {
    disconnected: "Disconnected",
    connecting: "Connecting",
    connected: "Connected",
    thinking: "Thinking",
    error: "Error",
  };

  return (
    <div
      ref={ref}
      className="absolute top-full left-0 right-0 z-50 mx-2 mt-1 rounded-lg border border-border bg-popover p-3 shadow-lg text-xs"
    >
      <div className="grid grid-cols-2 gap-y-1.5 gap-x-4">
        <span className="text-muted-foreground">Client</span>
        <span className="font-medium">{client?.name ?? "—"}</span>

        <span className="text-muted-foreground">Engagement</span>
        <span className="font-medium truncate">
          {engagement?.settings.description ?? "—"}
        </span>

        <span className="text-muted-foreground">Status</span>
        <span className="font-medium">{STATUS_LABELS[status] ?? status}</span>

        <span className="text-muted-foreground">Model</span>
        <span className="font-medium">{model ?? "—"}</span>

        <span className="text-muted-foreground">Duration</span>
        <span className="font-medium">{formatDuration(sessionStartedAt)}</span>

        <span className="text-muted-foreground">Cost</span>
        <span className="font-medium">${totalCostUsd.toFixed(4)}</span>

        <span className="text-muted-foreground">Session ID</span>
        <span className="font-mono text-[10px] truncate">{sessionId ?? "—"}</span>
      </div>
    </div>
  );
}
