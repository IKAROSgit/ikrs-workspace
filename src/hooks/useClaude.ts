import { useState, useCallback } from "react";
import { useEngagementStore } from "@/stores/engagementStore";
import { useAuth } from "@/providers/AuthProvider";
import {
  claudePreflight,
  scaffoldClaudeProject,
  launchClaude,
} from "@/lib/tauri-commands";
import type { ClaudeSession } from "@/types";

interface UseClaudeResult {
  session: ClaudeSession | null;
  isInstalled: boolean | null;
  launching: boolean;
  error: string | null;
  checkInstalled: () => Promise<void>;
  launch: () => Promise<void>;
}

export function useClaude(): UseClaudeResult {
  const [session, setSession] = useState<ClaudeSession | null>(null);
  const [isInstalled, setIsInstalled] = useState<boolean | null>(null);
  const [launching, setLaunching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { consultant } = useAuth();
  const engagement = useEngagementStore((s) =>
    s.engagements.find((e) => e.id === s.activeEngagementId)
  );
  const clients = useEngagementStore((s) => s.clients);
  const client = clients.find((c) => c.id === engagement?.clientId);

  const checkInstalled = useCallback(async () => {
    const result = await claudePreflight();
    setIsInstalled(result);
  }, []);

  const launch = useCallback(async () => {
    if (!engagement || !client || !consultant) return;
    setLaunching(true);
    setError(null);
    try {
      const projectPath = await scaffoldClaudeProject({
        clientSlug: client.slug,
        clientName: client.name,
        accountEmail: "",
        vaultPath: engagement.vault.path,
        timezone: engagement.settings.timezone,
        description: engagement.settings.description ?? "",
      });

      const pid = await launchClaude(projectPath, consultant.preferences.terminal);
      setSession({
        engagementId: engagement.id,
        pid,
        startedAt: new Date(),
        projectPath,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLaunching(false);
    }
  }, [engagement, client, consultant]);

  return { session, isInstalled, launching, error, checkInstalled, launch };
}
