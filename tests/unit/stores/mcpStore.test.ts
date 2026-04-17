import { describe, it, expect, beforeEach } from "vitest";
import { useMcpStore } from "@/stores/mcpStore";
import type { McpHealth } from "@/types";

describe("mcpStore", () => {
  beforeEach(() => {
    useMcpStore.setState({ servers: [] });
  });

  it("setServers populates server list", () => {
    const servers: McpHealth[] = [
      { type: "gmail", status: "healthy", lastPing: new Date(), restartCount: 0 },
      { type: "drive", status: "healthy", lastPing: new Date(), restartCount: 0 },
    ];
    useMcpStore.getState().setServers(servers);
    const afterSet = useMcpStore.getState().servers;
    expect(afterSet).toHaveLength(2);
    expect(afterSet[0]?.type).toBe("gmail");
  });

  it("setServerHealth updates individual server status", () => {
    useMcpStore.getState().setServers([
      { type: "gmail", status: "healthy", lastPing: new Date(), restartCount: 0 },
    ]);
    useMcpStore.getState().setServerHealth("gmail", "down");
    const afterUpdate = useMcpStore.getState().servers;
    expect(afterUpdate[0]?.status).toBe("down");
  });

  it("setServers with empty array clears all servers", () => {
    useMcpStore.getState().setServers([
      { type: "gmail", status: "healthy", lastPing: new Date(), restartCount: 0 },
    ]);
    useMcpStore.getState().setServers([]);
    expect(useMcpStore.getState().servers).toEqual([]);
  });

  it("consumer can find server by type", () => {
    useMcpStore.getState().setServers([
      { type: "gmail", status: "healthy", lastPing: new Date(), restartCount: 0 },
      { type: "drive", status: "down", lastPing: new Date(), restartCount: 0 },
    ]);
    const gmail = useMcpStore.getState().servers.find((s) => s.type === "gmail");
    expect(gmail?.status).toBe("healthy");
    const drive = useMcpStore.getState().servers.find((s) => s.type === "drive");
    expect(drive?.status).toBe("down");
  });
});
