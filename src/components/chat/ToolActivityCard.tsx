import { useState } from "react";
import { cn } from "@/lib/utils";
import { Loader2, CheckCircle, XCircle, ChevronDown, ChevronRight } from "lucide-react";
import type { ToolActivity } from "@/types/claude";

interface ToolActivityCardProps {
  tool: ToolActivity;
}

const TOOL_ICONS: Record<string, string> = {
  Write: "\u{1F4DD}",
  Edit: "\u{270F}\u{FE0F}",
  Read: "\u{1F4D6}",
  Glob: "\u{1F50D}",
  Grep: "\u{1F50D}",
  WebSearch: "\u{1F310}",
  WebFetch: "\u{1F310}",
};

export function ToolActivityCard({ tool }: ToolActivityCardProps) {
  const [expanded, setExpanded] = useState(false);
  const icon = TOOL_ICONS[tool.toolName] ?? "\u{2699}\u{FE0F}";
  const hasDetails = Boolean(tool.toolInput || tool.resultContent);

  return (
    <div
      className={cn(
        "rounded-md text-xs border border-border/50",
        "bg-muted/50",
        hasDetails && "cursor-pointer"
      )}
    >
      <div
        className="flex items-center gap-2 px-3 py-1.5"
        onClick={() => hasDetails && setExpanded(!expanded)}
      >
        {hasDetails && (
          expanded
            ? <ChevronDown size={12} className="text-muted-foreground shrink-0" />
            : <ChevronRight size={12} className="text-muted-foreground shrink-0" />
        )}
        <span>{icon}</span>
        <span className="flex-1 truncate">{tool.friendlyLabel}</span>
        {tool.status === "running" && (
          <Loader2 size={12} className="animate-spin text-muted-foreground" />
        )}
        {tool.status === "success" && (
          <CheckCircle size={12} className="text-green-500" />
        )}
        {tool.status === "error" && (
          <XCircle size={12} className="text-destructive" />
        )}
      </div>
      {expanded && (
        <div className="px-3 pb-2 space-y-1.5 border-t border-border/30 pt-1.5">
          {tool.toolInput && (
            <div>
              <span className="text-muted-foreground font-medium">Input:</span>
              <pre className="mt-0.5 p-1.5 rounded bg-background text-[10px] font-mono overflow-x-auto max-h-32 overflow-y-auto whitespace-pre-wrap break-all">
                {tool.toolInput}
              </pre>
            </div>
          )}
          {tool.resultContent && (
            <div>
              <span className="text-muted-foreground font-medium">Result:</span>
              <pre className="mt-0.5 p-1.5 rounded bg-background text-[10px] font-mono overflow-x-auto max-h-32 overflow-y-auto whitespace-pre-wrap break-all">
                {tool.resultContent}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
