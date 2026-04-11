import { create } from "zustand";
import type { McpHealth, McpServerType } from "@/types";

interface McpState {
  servers: McpHealth[];
  setServerHealth: (type: McpServerType, status: McpHealth["status"]) => void;
  setServers: (servers: McpHealth[]) => void;
}

export const useMcpStore = create<McpState>((set) => ({
  servers: [],
  setServerHealth: (type, status) =>
    set((state) => ({
      servers: state.servers.map((s) =>
        s.type === type ? { ...s, status, lastPing: new Date() } : s
      ),
    })),
  setServers: (servers) => set({ servers }),
}));
