import { useState, useEffect, useCallback } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { checkSkillUpdates, applySkillUpdates } from "@/lib/tauri-commands";
import {
  SKILL_DOMAINS,
  type SkillDomain,
  type SkillUpdateStatus,
  type SkillUpdateParams,
} from "@/types/skills";

interface SkillStatusPanelProps {
  /** Parameters needed to check/apply skill updates. */
  updateParams: SkillUpdateParams | null;
}

/** Human-readable labels for each skill domain. */
const DOMAIN_LABELS: Record<SkillDomain, string> = {
  communications: "Communications",
  planning: "Planning",
  creative: "Creative",
  operations: "Operations",
  legal: "Legal",
  finance: "Finance",
  research: "Research",
  talent: "Talent & Entertainment",
};

/** Icons for each domain (unicode/emoji-free, text-based). */
const DOMAIN_ICONS: Record<SkillDomain, string> = {
  communications: "COM",
  planning: "PLN",
  creative: "CRE",
  operations: "OPS",
  legal: "LEG",
  finance: "FIN",
  research: "RES",
  talent: "TAL",
};

export function SkillStatusPanel({ updateParams }: SkillStatusPanelProps) {
  const [status, setStatus] = useState<SkillUpdateStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchStatus = useCallback(async () => {
    if (!updateParams) return;
    setLoading(true);
    setError(null);
    try {
      const result = await checkSkillUpdates(updateParams);
      setStatus(result);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [updateParams]);

  useEffect(() => {
    fetchStatus();
  }, [fetchStatus]);

  const handleUpdateAll = async () => {
    if (!updateParams || !status) return;
    setUpdating(true);
    setError(null);
    try {
      await applySkillUpdates(updateParams, status.updatable_folders);
      await fetchStatus(); // Refresh status after update
    } catch (err) {
      setError(String(err));
    } finally {
      setUpdating(false);
    }
  };

  if (!updateParams) {
    return null;
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between">
          <span>Skill Domains</span>
          {status?.updates_available && (
            <Badge variant="outline" className="text-xs">
              v{status.installed_version} &rarr; v{status.bundled_version}
            </Badge>
          )}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {loading && <p className="text-muted-foreground text-sm">Checking skills...</p>}

        {error && <p className="text-red-500 text-sm">{error}</p>}

        {status && !loading && (
          <>
            <div className="grid grid-cols-2 gap-2">
              {SKILL_DOMAINS.map((domain) => {
                const isCustom = status.customized_folders.includes(domain);
                const isUpdatable = status.updatable_folders.includes(domain);

                return (
                  <div
                    key={domain}
                    className="flex items-center gap-2 p-2 rounded border text-sm"
                  >
                    <span className="font-mono text-xs text-muted-foreground w-8">
                      {DOMAIN_ICONS[domain]}
                    </span>
                    <span className="flex-1">{DOMAIN_LABELS[domain]}</span>
                    {isCustom && (
                      <Badge variant="secondary" className="text-xs">
                        custom
                      </Badge>
                    )}
                    {isUpdatable && status.updates_available && (
                      <Badge variant="default" className="text-xs">
                        update
                      </Badge>
                    )}
                    {!isCustom && !isUpdatable && !status.updates_available && (
                      <Badge variant="outline" className="text-xs">
                        current
                      </Badge>
                    )}
                  </div>
                );
              })}
            </div>

            {status.updates_available && status.updatable_folders.length > 0 && (
              <Button
                onClick={handleUpdateAll}
                disabled={updating}
                size="sm"
                className="w-full"
              >
                {updating
                  ? "Updating..."
                  : `Update ${status.updatable_folders.length} skill${status.updatable_folders.length === 1 ? "" : "s"}`}
              </Button>
            )}

            {status.updates_available && status.customized_folders.length > 0 && (
              <p className="text-muted-foreground text-xs">
                {status.customized_folders.length} skill
                {status.customized_folders.length === 1 ? " has" : "s have"} custom
                CLAUDE.md files and will not be overwritten.
              </p>
            )}

            {!status.updates_available && (
              <p className="text-muted-foreground text-xs">
                All skills are at version {status.installed_version}.
              </p>
            )}
          </>
        )}
      </CardContent>
    </Card>
  );
}
