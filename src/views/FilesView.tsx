import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  RefreshCw,
  FolderOpen,
  File,
  FileText,
  Image,
  Search,
} from "lucide-react";
import { useDrive } from "@/hooks/useDrive";
import { useEngagementStore } from "@/stores/engagementStore";
import { OfflineBanner } from "@/components/OfflineBanner";

function FileIcon({ mimeType }: { mimeType: string }) {
  if (mimeType.startsWith("image/"))
    return <Image size={16} className="text-purple-400" />;
  if (mimeType.includes("folder"))
    return <FolderOpen size={16} className="text-yellow-400" />;
  if (mimeType.includes("document") || mimeType.includes("text"))
    return <FileText size={16} className="text-blue-400" />;
  return <File size={16} className="text-muted-foreground" />;
}

export default function FilesView() {
  const { files, loading, error, isConnected, refresh, search } = useDrive();
  const activeEngagementId = useEngagementStore((s) => s.activeEngagementId);
  const [searchQuery, setSearchQuery] = useState("");

  if (!activeEngagementId) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
        <FolderOpen size={48} className="mb-4 opacity-50" />
        <p>Select an engagement to browse files.</p>
      </div>
    );
  }

  if (!isConnected) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
        <FolderOpen size={48} className="mb-4 opacity-50" />
        <p>Connect a Google account in Settings to browse Drive.</p>
      </div>
    );
  }

  const handleSearch = () => {
    if (searchQuery.trim()) search(searchQuery.trim());
  };

  return (
    <div className="flex flex-col h-full">
      <OfflineBanner feature="Google Drive" />
      <div className="flex items-center gap-2 px-4 py-2 border-b border-border">
        <div className="relative flex-1">
          <Search
            className="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground"
            size={14}
          />
          <Input
            placeholder="Search files..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleSearch()}
            className="h-8 pl-8 text-sm"
          />
        </div>
        <Button variant="ghost" size="sm" onClick={refresh} disabled={loading}>
          <RefreshCw size={14} className={loading ? "animate-spin" : ""} />
        </Button>
      </div>

      {error && (
        <div className="px-4 py-2 bg-destructive/10 text-destructive text-sm">
          {error}
        </div>
      )}

      <ScrollArea className="flex-1">
        {files.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
            <p className="text-sm">
              {loading ? "Loading files..." : "No files found."}
            </p>
          </div>
        ) : (
          <div className="divide-y divide-border">
            {files.map((file) => (
              <a
                key={file.id}
                href={file.webViewLink ?? undefined}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center gap-3 px-4 py-2 hover:bg-accent/50 cursor-pointer"
                title={file.name}
              >
                <FileIcon mimeType={file.mimeType} />
                <span className="flex-1 text-sm truncate">{file.name}</span>
                {file.size && (
                  <span className="text-xs text-muted-foreground">
                    {formatSize(file.size)}
                  </span>
                )}
                <span className="text-xs text-muted-foreground whitespace-nowrap">
                  {formatDriveDate(file.modifiedTime)}
                </span>
              </a>
            ))}
          </div>
        )}
      </ScrollArea>
    </div>
  );
}

/** Drive `modifiedTime` is RFC 3339 — format compactly. */
function formatDriveDate(raw: string): string {
  if (!raw) return "";
  const d = new Date(raw);
  if (Number.isNaN(d.getTime())) return raw;
  const now = new Date();
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  if (sameDay) {
    return d.toLocaleTimeString(undefined, {
      hour: "numeric",
      minute: "2-digit",
    });
  }
  const sameYear = d.getFullYear() === now.getFullYear();
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: sameYear ? undefined : "numeric",
  });
}

function formatSize(raw: string | null | undefined): string {
  if (!raw) return "";
  const n = parseInt(raw, 10);
  if (Number.isNaN(n)) return "";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}
