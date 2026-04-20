import { useState, useEffect } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  FileText,
  FolderOpen,
  RefreshCw,
  Archive,
  RotateCcw,
  X,
} from "lucide-react";
import { useNotes } from "@/hooks/useNotes";
import { useEngagementStore } from "@/stores/engagementStore";
import { archiveVault, restoreVault } from "@/lib/tauri-commands";
import { OfflineBanner } from "@/components/OfflineBanner";

export default function NotesView() {
  const { files, loading, error, isConnected, refresh, readContent } =
    useNotes();
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const engagement = useEngagementStore((s) =>
    s.engagements.find((e) => e.id === s.activeEngagementId),
  );
  const clients = useEngagementStore((s) => s.clients);
  const client = clients.find((c) => c.id === engagement?.clientId);

  const [openRelPath, setOpenRelPath] = useState<string | null>(null);
  const [openContent, setOpenContent] = useState<string | null>(null);
  const [openLoading, setOpenLoading] = useState(false);
  const [openError, setOpenError] = useState<string | null>(null);

  // Reset preview on engagement switch
  useEffect(() => {
    setOpenRelPath(null);
    setOpenContent(null);
    setOpenError(null);
  }, [activeEngagementId]);

  const openFile = async (relPath: string) => {
    setOpenRelPath(relPath);
    setOpenContent(null);
    setOpenError(null);
    setOpenLoading(true);
    try {
      const content = await readContent(relPath);
      setOpenContent(content);
    } catch (e) {
      setOpenError(e instanceof Error ? e.message : String(e));
    } finally {
      setOpenLoading(false);
    }
  };

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
      <OfflineBanner feature="Notes" />
      {/* Notes are read directly from the vault filesystem — they
          work without an active Claude session. The old 'requires
          Claude session' banner was based on the MCP bridge that
          no longer gates this view. */}
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
        <div className="flex-1 flex overflow-hidden">
          <ScrollArea className="flex-1 max-w-md border-r border-border">
            {files.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
                <p className="text-sm">
                  {isConnected
                    ? "No notes yet. Ask Claude to create some."
                    : "Vault not created yet for this engagement."}
                </p>
              </div>
            ) : (
              <div className="divide-y divide-border">
                {files.map((file) => (
                  <button
                    key={file.path}
                    type="button"
                    onClick={() =>
                      !file.is_directory && openFile(file.rel_path)
                    }
                    disabled={file.is_directory}
                    className={`w-full text-left flex items-center gap-3 px-4 py-2 hover:bg-accent/50 ${
                      openRelPath === file.rel_path ? "bg-accent/70" : ""
                    } ${file.is_directory ? "cursor-default opacity-80" : "cursor-pointer"}`}
                    title={file.rel_path}
                  >
                    {file.is_directory ? (
                      <FolderOpen size={16} className="text-yellow-400" />
                    ) : (
                      <FileText size={16} className="text-blue-400" />
                    )}
                    <span className="text-sm flex-1 truncate">{file.name}</span>
                    {!file.is_directory && (
                      <span className="text-xs text-muted-foreground">
                        {file.size_bytes < 1024
                          ? `${file.size_bytes} B`
                          : `${Math.round(file.size_bytes / 1024)} KB`}
                      </span>
                    )}
                  </button>
                ))}
              </div>
            )}
          </ScrollArea>
          <div className="flex-1 flex flex-col overflow-hidden">
            {openRelPath ? (
              <>
                <div className="flex items-center gap-2 px-4 py-2 border-b border-border">
                  <FileText size={14} className="text-blue-400" />
                  <span className="text-xs font-medium flex-1 truncate">
                    {openRelPath}
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => {
                      setOpenRelPath(null);
                      setOpenContent(null);
                      setOpenError(null);
                    }}
                  >
                    <X size={14} />
                  </Button>
                </div>
                <ScrollArea className="flex-1 p-4">
                  {openLoading && (
                    <p className="text-sm text-muted-foreground">Loading…</p>
                  )}
                  {openError && (
                    <div className="text-sm text-destructive bg-destructive/10 p-3 rounded">
                      {openError}
                    </div>
                  )}
                  {openContent != null && !openLoading && !openError && (
                    <div className="prose prose-sm dark:prose-invert max-w-none">
                      <ReactMarkdown remarkPlugins={[remarkGfm]}>
                        {openContent}
                      </ReactMarkdown>
                    </div>
                  )}
                </ScrollArea>
              </>
            ) : (
              <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm">
                Select a note to preview
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
