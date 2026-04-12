import { useCallback } from "react";
import { useEngagementStore } from "@/stores/engagementStore";
import { useMcpStore } from "@/stores/mcpStore";
import {
  killAllMcp,
  spawnMcp,
  getCredential,
  makeKeychainKey,
  createVault,
} from "@/lib/tauri-commands";
import type { McpServerType, McpHealth } from "@/types";

interface McpConfig {
  type: McpServerType;
  command: string;
  args: string[];
}

const MCP_CONFIGS: McpConfig[] = [
  { type: "gmail", command: "npx", args: ["@shinzolabs/gmail-mcp@1.7.4"] },
  { type: "calendar", command: "npx", args: ["@cocal/google-calendar-mcp@2.6.1"] },
  { type: "drive", command: "npx", args: ["@piotr-agier/google-drive-mcp@2.0.2"] },
];

/**
 * MCP lifecycle management for engagements.
 * Session switching is handled by useWorkspaceSession — this hook
 * only manages MCP servers (until Phase 3b retires app-side spawning).
 */
export function useEngagement() {
  const engagements = useEngagementStore((s) => s.engagements);
  const clients = useEngagementStore((s) => s.clients);
  const setServers = useMcpStore((s) => s.setServers);

  const refreshMcpServers = useCallback(
    async (engagementId: string) => {
      await killAllMcp();

      const key = makeKeychainKey(engagementId, "google");
      const token = await getCredential(key);

      const engagement = engagements.find((e) => e.id === engagementId);
      const client = clients.find((c) => c.id === engagement?.clientId);
      if (client) {
        await createVault(client.slug);
      }

      if (token) {
        const newServers: McpHealth[] = [];
        for (const config of MCP_CONFIGS) {
          try {
            const pid = await spawnMcp({
              server_type: config.type,
              command: config.command,
              args: config.args,
              env: { GOOGLE_ACCESS_TOKEN: token },
            });
            newServers.push({
              type: config.type,
              status: "healthy",
              pid,
              lastPing: new Date(),
              restartCount: 0,
            });
          } catch {
            newServers.push({
              type: config.type,
              status: "down",
              restartCount: 0,
            });
          }
        }
        if (client) {
          try {
            const home = await import("@tauri-apps/api/path").then((m) => m.homeDir());
            const vaultPath = `${home}.ikrs-workspace/vaults/${client.slug}`;
            const pid = await spawnMcp({
              server_type: "obsidian",
              command: "npx",
              args: ["@bitbonsai/mcpvault@1.3.0", vaultPath],
              env: {},
            });
            newServers.push({
              type: "obsidian",
              status: "healthy",
              pid,
              lastPing: new Date(),
              restartCount: 0,
            });
          } catch {
            newServers.push({
              type: "obsidian",
              status: "down",
              restartCount: 0,
            });
          }
        }
        setServers(newServers);
      } else {
        setServers([]);
      }
    },
    [setServers, engagements, clients],
  );

  return { refreshMcpServers };
}
