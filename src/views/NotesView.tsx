import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  FileText,
  FolderOpen,
  RefreshCw,
  Archive,
  RotateCcw,
} from "lucide-react";
import { useNotes } from "@/hooks/useNotes";
import { useEngagementStore } from "@/stores/engagementStore";
import { useClaudeStore } from "@/stores/claudeStore";
import { archiveVault, restoreVault } from "@/lib/tauri-commands";
import { OfflineBanner } from "@/components/OfflineBanner";

export default function NotesView() {
  const { files, loading, error, isConnected, refresh } = useNotes();
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const engagement = useEngagementStore((s) =>
    s.engagements.find((e) => e.id === s.activeEngagementId),
  );
  const clients = useEngagementStore((s) => s.clients);
  const client = clients.find((c) => c.id === engagement?.clientId);
  const claudeStatus = useClaudeStore((s) => s.status);

  if (!activeEngagementId || !engagement) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
        <FileText size={48} className="mb-4 opacity-50" />
        <p>Select an engagement to view notes.</p>
      </div>
    );
  }

  const isArchived = engagement.vault.status === "archived";

  return (
    <div className="flex flex-col h-full">
      <OfflineBanner feature="Notes (Obsidian MCP)" />
      {claudeStatus !== "connected" && (
        <div className="px-4 py-2 bg-muted text-muted-foreground text-sm border-b border-border">
          Notes require an active Claude session. Connect to Claude to access vault files.
        </div>
      )}
      <div className="flex items-center justify-between px-4 py-2 border-b border-border">
        <h2 className="text-sm font-semibold">
          Notes — {client?.name ?? "Unknown Client"}
        </h2>
        <div className="flex gap-1">
          {!isArchived && (
            <>
              <Button
                variant="ghost"
                size="sm"
                onClick={refresh}
                disabled={loading}
              >
                <RefreshCw
                  size={14}
                  className={loading ? "animate-spin" : ""}
                />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => client && archiveVault(client.slug)}
                title="Archive vault"
              >
                <Archive size={14} />
              </Button>
            </>
          )}
          {isArchived && engagement.vault.archivePath && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => restoreVault(engagement.vault.archivePath!)}
              title="Restore vault"
            >
              <RotateCcw size={14} />
            </Button>
          )}
        </div>
      </div>

      {error && (
        <div className="px-4 py-2 bg-destructive/10 text-destructive text-sm">
          {error}
        </div>
      )}

      {isArchived ? (
        <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
          <Archive size={32} className="mb-2 opacity-50" />
          <p className="text-sm">
            Vault archived — click restore to reactivate.
          </p>
        </div>
      ) : (
        <ScrollArea className="flex-1">
          {files.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
              <p className="text-sm">
                {isConnected
                  ? "Vault empty. Create notes to get started."
                  : "Vault created. MCP connection pending."}
              </p>
            </div>
          ) : (
            <div className="divide-y divide-border">
              {files.map((file) => (
                <div
                  key={file.path}
                  className="flex items-center gap-3 px-4 py-2 hover:bg-accent/50 cursor-pointer"
                >
                  {file.isDirectory ? (
                    <FolderOpen size={16} />
                  ) : (
                    <FileText size={16} />
                  )}
                  <span className="text-sm">{file.name}</span>
                </div>
              ))}
            </div>
          )}
        </ScrollArea>
      )}
    </div>
  );
}
