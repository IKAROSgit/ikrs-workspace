import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { useEngagementStore } from "@/stores/engagementStore";

interface OrphanVault {
  slug: string;
  path: string;
  task_count: number;
  last_modified: string | null;
}

interface ImportResult {
  imported: number;
  skipped: number;
}

export function OrphanVaultsCard() {
  const [orphans, setOrphans] = useState<OrphanVault[]>([]);
  const [importing, setImporting] = useState<string | null>(null);
  const [selectedDest, setSelectedDest] = useState<Record<string, string>>({});

  const engagements = useEngagementStore((s) => s.engagements);
  const clients = useEngagementStore((s) => s.clients);

  useEffect(() => {
    const scan = async () => {
      try {
        const knownSlugs = clients.map((c) => c.slug);
        const result = await invoke<OrphanVault[]>("scan_orphan_vaults", {
          knownSlugs,
        });
        setOrphans(result);
      } catch (e) {
        console.warn("[orphan-scan] failed:", e);
      }
    };
    void scan();
  }, [clients]);

  if (orphans.length === 0) return null;

  const handleImport = async (orphan: OrphanVault) => {
    const destEngId = selectedDest[orphan.slug];
    if (!destEngId) return;

    const destEng = engagements.find((e) => e.id === destEngId);
    const destClient = destEng
      ? clients.find((c) => c.id === destEng.clientId)
      : null;
    const destSlug = destClient?.slug;
    if (!destSlug) return;

    const confirmed = window.confirm(
      `Import ${orphan.task_count} tasks from "${orphan.slug}" into "${destSlug}"?\n\n` +
        "Existing tasks with the same filename will be skipped.\n" +
        "This cannot be undone from the UI.",
    );
    if (!confirmed) return;

    setImporting(orphan.slug);
    try {
      const result = await invoke<ImportResult>("import_orphan_vault", {
        sourceSlug: orphan.slug,
        destSlug,
      });
      alert(
        `Import complete: ${result.imported} imported, ${result.skipped} skipped.`,
      );
      // Re-scan
      const knownSlugs = clients.map((c) => c.slug);
      const updated = await invoke<OrphanVault[]>("scan_orphan_vaults", {
        knownSlugs,
      });
      setOrphans(updated);
    } catch (e) {
      alert(`Import failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setImporting(null);
    }
  };

  return (
    <Card className="border-yellow-500/50">
      <CardHeader>
        <CardTitle className="text-yellow-600">Orphan Vaults</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <p className="text-sm text-muted-foreground">
          These vault folders don't match any active engagement. Tasks inside
          them are invisible to the Kanban board.
        </p>
        {orphans.map((o) => (
          <div
            key={o.slug}
            className="flex items-center gap-3 p-2 rounded bg-muted/50"
          >
            <div className="flex-1 min-w-0">
              <p className="font-mono text-sm truncate">{o.slug}</p>
              <p className="text-xs text-muted-foreground">
                {o.task_count} task{o.task_count !== 1 ? "s" : ""}
              </p>
            </div>
            <select
              className="text-sm border rounded px-2 py-1"
              value={selectedDest[o.slug] ?? ""}
              onChange={(e) =>
                setSelectedDest((prev) => ({
                  ...prev,
                  [o.slug]: e.target.value,
                }))
              }
            >
              <option value="">Import into...</option>
              {engagements.map((eng) => {
                const client = clients.find((c) => c.id === eng.clientId);
                return (
                  <option key={eng.id} value={eng.id}>
                    {client?.name ?? eng.id}
                  </option>
                );
              })}
            </select>
            <Button
              size="sm"
              variant="outline"
              disabled={
                !selectedDest[o.slug] || importing === o.slug
              }
              onClick={() => handleImport(o)}
            >
              {importing === o.slug ? "Importing..." : "Import"}
            </Button>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}
