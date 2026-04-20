import { useState } from "react";
import { cn } from "@/lib/utils";
import type { ClaudeSessionStatus } from "@/types/claude";
import { SessionDetailsModal } from "@/components/chat/SessionDetailsModal";

interface SessionIndicatorProps {
  status: ClaudeSessionStatus;
  model: string | null;
  costUsd: number;
  switching?: boolean;
}

const STATUS_CONFIG: Record<
  ClaudeSessionStatus,
  { color: string; label: string }
> = {
  disconnected: { color: "bg-gray-400", label: "Disconnected" },
  connecting: { color: "bg-yellow-400 animate-pulse", label: "Connecting..." },
  connected: { color: "bg-green-500", label: "Connected" },
  thinking: { color: "bg-yellow-400 animate-pulse", label: "Thinking..." },
  error: { color: "bg-red-500", label: "Error" },
};

export function SessionIndicator({
  status,
  model,
  costUsd,
  switching,
}: SessionIndicatorProps) {
  const [showDetails, setShowDetails] = useState(false);
  const config = STATUS_CONFIG[status];
  const label = switching ? "Switching..." : config.label;

  return (
    <div className="relative">
      <div
        className="flex items-center gap-3 px-4 py-2 border-b border-border text-xs text-muted-foreground cursor-pointer hover:bg-muted/50 transition-colors"
        onClick={() => setShowDetails(!showDetails)}
      >
        <div className="flex items-center gap-1.5">
          <div className={cn("w-2 h-2 rounded-full", switching ? "bg-yellow-400 animate-pulse" : config.color)} />
          <span>{label}</span>
        </div>
        {model && <span className="hidden sm:inline">{model}</span>}
        {costUsd > 0 && (
          <span
            className="ml-auto"
            title="Usage meter — Claude Code reports what this session would have cost on the pay-per-token API. Your Max subscription absorbs this up to your monthly allowance; it's not a separate bill."
          >
            ${costUsd.toFixed(4)}
            <span className="text-[10px] opacity-70 ml-0.5">/Max</span>
          </span>
        )}
      </div>
      {showDetails && (
        <SessionDetailsModal onClose={() => setShowDetails(false)} />
      )}
    </div>
  );
}
