import { useEffect, useRef } from "react";
import { useClaudeStore } from "@/stores/claudeStore";
import { useCostLedgerStore } from "@/stores/costLedgerStore";
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

function todayKey(): string {
  const d = new Date();
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
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

  // Subscribe to the ledger so the panel updates live if a turn
  // completes while it's open. Reading the map (not a derived
  // function) is what triggers re-renders; the selector functions
  // are stable refs and wouldn't.
  const ledger = useCostLedgerStore((s) => s.engagements);
  const todayAcrossAll = useCostLedgerStore((s) => s.todayTotal)();
  const todayThisEngagement = activeEngagementId
    ? (ledger[activeEngagementId]?.byDay[todayKey()] ?? 0)
    : 0;
  const allTimeThisEngagement = activeEngagementId
    ? (ledger[activeEngagementId]?.allTime ?? 0)
    : 0;

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

        <span className="text-muted-foreground">Usage</span>
        <span className="font-medium" title="Max-subscription usage meter — Claude Code reports what this session would have cost on the pay-per-token API. Your Max subscription absorbs it up to your monthly allowance; this is not a separate bill.">
          ${totalCostUsd.toFixed(4)}
          <span className="text-muted-foreground ml-1 text-[10px]">/ Max</span>
        </span>

        <span className="text-muted-foreground">Session ID</span>
        <span className="font-mono text-[10px] truncate">{sessionId ?? "—"}</span>
      </div>

      <div className="mt-3 pt-3 border-t border-border">
        <div className="text-[10px] uppercase tracking-wider text-muted-foreground mb-1.5">
          Usage rollups <span className="normal-case">· persisted locally</span>
        </div>
        <div className="grid grid-cols-2 gap-y-1.5 gap-x-4">
          <span className="text-muted-foreground">Today (all engagements)</span>
          <span className="font-medium">${todayAcrossAll.toFixed(4)}</span>

          <span className="text-muted-foreground">Today (this engagement)</span>
          <span className="font-medium">${todayThisEngagement.toFixed(4)}</span>

          <span className="text-muted-foreground">This engagement · all-time</span>
          <span className="font-medium">${allTimeThisEngagement.toFixed(4)}</span>
        </div>
        <p className="text-[10px] text-muted-foreground mt-2 leading-relaxed">
          Meter only — your Max subscription absorbs this until your
          monthly allowance is spent. Rollups survive reconnects and
          app restarts; clearing browser storage resets them.
        </p>
      </div>
    </div>
  );
}
