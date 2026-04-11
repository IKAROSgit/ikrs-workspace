import { create } from "zustand";
import type { McpHealth, McpServerType } from "@/types";

interface McpState {
  servers: McpHealth[];
  setServerHealth: (type: McpServerType, status: McpHealth["status"]) => void;
  setServers: (servers: McpHealth[]) => void;
  getServer: (type: McpServerType) => McpHealth | undefined;
}

export const useMcpStore = create<McpState>((set, get) => ({
  servers: [],
  setServerHealth: (type, status) =>
    set((state) => ({
      servers: state.servers.map((s) =>
        s.type === type ? { ...s, status, lastPing: new Date() } : s
      ),
    })),
  setServers: (servers) => set({ servers }),
  getServer: (type) => get().servers.find((s) => s.type === type),
}));
