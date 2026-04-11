import { cn } from "@/lib/utils";
import type { ClaudeSessionStatus } from "@/types/claude";

interface SessionIndicatorProps {
  status: ClaudeSessionStatus;
  model: string | null;
  costUsd: number;
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
}: SessionIndicatorProps) {
  const config = STATUS_CONFIG[status];

  return (
    <div className="flex items-center gap-3 px-4 py-2 border-b border-border text-xs text-muted-foreground">
      <div className="flex items-center gap-1.5">
        <div className={cn("w-2 h-2 rounded-full", config.color)} />
        <span>{config.label}</span>
      </div>
      {model && <span className="hidden sm:inline">{model}</span>}
      {costUsd > 0 && (
        <span className="ml-auto">${costUsd.toFixed(4)}</span>
      )}
    </div>
  );
}
