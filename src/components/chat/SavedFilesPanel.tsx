import { CheckCircle2, AlertTriangle, FileText } from "lucide-react";
import { useClaudeStore } from "@/stores/claudeStore";
import { cn } from "@/lib/utils";
import type { WriteVerificationEntry } from "@/types/claude";

/**
 * Ground-truth ledger of every file Claude wrote / edited this
 * session. Backed by `claudeStore.writeVerifications`, which the
 * Rust stream parser populates AFTER stat'ing the file post-tool_use.
 *
 * Green check = file exists on disk with the expected content.
 * Red alert  = Claude claimed success but the stat disagreed
 *              (the lie-class-of-bug that dropped Moe's triaged
 *              tracker + transcript on 2026-04-20).
 *
 * This panel is the app's independent answer to "did Claude actually
 * save what it said it saved?" — users never have to take Claude's
 * word for it.
 */
export function SavedFilesPanel() {
  const entries = useClaudeStore((s) => s.writeVerifications);
  if (entries.length === 0) return null;

  return (
    <div className="border-l border-border bg-background/60 w-72 flex-shrink-0 overflow-hidden flex flex-col">
      <div className="px-3 py-2 border-b border-border text-xs font-semibold text-muted-foreground">
        Saved this session
      </div>
      <div className="flex-1 overflow-y-auto divide-y divide-border">
        {entries.map((e, i) => (
          <SavedFileRow key={`${e.tool_id}-${i}`} entry={e} />
        ))}
      </div>
    </div>
  );
}

function SavedFileRow({ entry }: { entry: WriteVerificationEntry }) {
  const verified = entry.verified;
  const lie = entry.claude_claimed_success && !verified;

  const shortPath = entry.path.split("/").slice(-2).join("/");
  const sizeLabel =
    entry.size_bytes != null
      ? entry.size_bytes < 1024
        ? `${entry.size_bytes} B`
        : `${Math.round(entry.size_bytes / 1024)} KB`
      : "—";
  const timeLabel = entry.timestamp.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });

  return (
    <div
      className={cn(
        "px-3 py-2 flex items-start gap-2 text-xs",
        lie && "bg-destructive/10",
      )}
      title={entry.path}
    >
      {verified ? (
        <CheckCircle2
          size={14}
          className="text-green-600 dark:text-green-500 mt-0.5 flex-shrink-0"
        />
      ) : lie ? (
        <AlertTriangle
          size={14}
          className="text-destructive mt-0.5 flex-shrink-0"
        />
      ) : (
        <FileText size={14} className="text-muted-foreground mt-0.5 flex-shrink-0" />
      )}
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium">{shortPath}</div>
        <div className="text-muted-foreground text-[11px] flex items-center gap-2">
          <span>{entry.tool_name}</span>
          <span>{sizeLabel}</span>
          <span>{timeLabel}</span>
        </div>
        {lie && (
          <div className="text-destructive text-[11px] mt-1">
            Not saved — {entry.reason ?? "file missing on disk"}
          </div>
        )}
        {!verified && !lie && entry.reason && (
          <div className="text-muted-foreground text-[11px] mt-1">
            {entry.reason}
          </div>
        )}
      </div>
    </div>
  );
}
