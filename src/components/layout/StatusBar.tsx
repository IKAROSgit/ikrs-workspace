import type { McpHealth } from "@/types";

interface StatusBarProps {
  connectedEmail?: string;
  mcpStatuses: McpHealth[];
  isOnline: boolean;
}

function HealthDot({ status }: { status: McpHealth["status"] }) {
  const color = {
    healthy: "bg-green-500",
    reconnecting: "bg-yellow-500 animate-pulse",
    down: "bg-red-500",
    stopped: "bg-gray-500",
  }[status];
  return <span className={`w-2 h-2 rounded-full ${color}`} />;
}

export function StatusBar({ connectedEmail, mcpStatuses, isOnline }: StatusBarProps) {
  return (
    <footer className="flex items-center h-6 px-4 border-t border-border bg-muted text-xs text-muted-foreground gap-4">
      {connectedEmail ? (
        <span>Connected: {connectedEmail}</span>
      ) : (
        <span>No account linked</span>
      )}
      <span className="flex-1" />
      {!isOnline && <span className="text-yellow-500">Offline</span>}
      <div className="flex items-center gap-2">
        {mcpStatuses.map((mcp) => (
          <div key={mcp.type} className="flex items-center gap-1">
            <HealthDot status={mcp.status} />
            <span className="capitalize">{mcp.type}</span>
          </div>
        ))}
      </div>
    </footer>
  );
}
