import { create } from "zustand";
import type { ChatMessage, ToolActivity, ClaudeSessionStatus } from "@/types/claude";

interface ClaudeState {
  sessionId: string | null;
  status: ClaudeSessionStatus;
  messages: ChatMessage[];
  activeTools: ToolActivity[];
  totalCostUsd: number;
  error: string | null;
  availableTools: string[];
  model: string | null;

  setSessionReady: (sessionId: string, tools: string[], model: string) => void;
  addUserMessage: (text: string) => void;
  addTextDelta: (messageId: string, text: string) => void;
  startTool: (toolId: string, toolName: string, friendlyLabel: string) => void;
  endTool: (toolId: string, success: boolean, summary: string) => void;
  completeTurn: (costUsd: number, durationMs: number) => void;
  setError: (message: string) => void;
  setDisconnected: (reason: string) => void;
  reset: () => void;
}

const initialState = {
  sessionId: null as string | null,
  status: "disconnected" as ClaudeSessionStatus,
  messages: [] as ChatMessage[],
  activeTools: [] as ToolActivity[],
  totalCostUsd: 0,
  error: null as string | null,
  availableTools: [] as string[],
  model: null as string | null,
};

export const useClaudeStore = create<ClaudeState>()((set) => ({
  ...initialState,

  setSessionReady: (sessionId, tools, model) =>
    set({
      sessionId,
      status: "connected",
      availableTools: tools,
      model,
      error: null,
    }),

  addUserMessage: (text) =>
    set((state) => ({
      messages: [
        ...state.messages,
        {
          id: `user_${Date.now()}`,
          role: "user" as const,
          text,
          timestamp: new Date(),
          isStreaming: false,
        },
      ],
      status: "thinking",
    })),

  addTextDelta: (messageId, text) =>
    set((state) => {
      const existing = state.messages.find(
        (m) => m.id === messageId && m.role === "assistant"
      );
      if (existing) {
        return {
          messages: state.messages.map((m) =>
            m.id === messageId
              ? { ...m, text: m.text + text, isStreaming: true }
              : m
          ),
        };
      }
      return {
        messages: [
          ...state.messages,
          {
            id: messageId,
            role: "assistant" as const,
            text,
            timestamp: new Date(),
            isStreaming: true,
          },
        ],
      };
    }),

  startTool: (toolId, toolName, friendlyLabel) =>
    set((state) => ({
      activeTools: [
        ...state.activeTools,
        {
          toolId,
          toolName,
          friendlyLabel,
          status: "running" as const,
          startedAt: new Date(),
        },
      ],
    })),

  endTool: (toolId, success, summary) =>
    set((state) => ({
      activeTools: state.activeTools.map((t) =>
        t.toolId === toolId
          ? {
              ...t,
              status: (success ? "success" : "error") as "success" | "error",
              summary,
              completedAt: new Date(),
            }
          : t
      ),
    })),

  completeTurn: (costUsd, _durationMs) =>
    set((state) => ({
      status: state.sessionId ? "connected" : "disconnected",
      totalCostUsd: state.totalCostUsd + costUsd,
      messages: state.messages.map((m) =>
        m.isStreaming ? { ...m, isStreaming: false } : m
      ),
    })),

  setError: (message) =>
    set({
      status: "error",
      error: message,
    }),

  setDisconnected: (reason) =>
    set({
      status: "disconnected",
      sessionId: null,
      error: reason || null,
    }),

  reset: () => set(initialState),
}));
