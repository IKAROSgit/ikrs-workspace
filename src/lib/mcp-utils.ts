import type { McpHealth, McpServerType } from "@/types";

const MCP_PREFIX_MAP: Record<string, McpServerType> = {
  gmail: "gmail",
  calendar: "calendar",
  drive: "drive",
  obsidian: "obsidian",
};

export function extractMcpServers(tools: string[]): McpHealth[] {
  const found = new Set<McpServerType>();
  for (const tool of tools) {
    const match = tool.match(/^mcp__(\w+)__/);
    if (match) {
      const serverType = MCP_PREFIX_MAP[match[1]];
      if (serverType) found.add(serverType);
    }
  }
  return Array.from(found).map((type) => ({
    type,
    status: "healthy" as const,
    lastPing: new Date(),
    restartCount: 0,
  }));
}
