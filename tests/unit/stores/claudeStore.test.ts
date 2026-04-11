import { describe, it, expect, beforeEach } from "vitest";
import { useClaudeStore } from "@/stores/claudeStore";

describe("claudeStore", () => {
  beforeEach(() => {
    useClaudeStore.getState().reset();
  });

  it("starts disconnected with empty messages", () => {
    const state = useClaudeStore.getState();
    expect(state.status).toBe("disconnected");
    expect(state.messages).toEqual([]);
    expect(state.sessionId).toBeNull();
  });

  it("setSessionReady transitions to connected", () => {
    useClaudeStore.getState().setSessionReady("sess-1", ["Read", "Write"], "claude-sonnet-4-6");
    const state = useClaudeStore.getState();
    expect(state.status).toBe("connected");
    expect(state.sessionId).toBe("sess-1");
  });

  it("addUserMessage appends a user message", () => {
    useClaudeStore.getState().addUserMessage("Hello Claude");
    const state = useClaudeStore.getState();
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0]!.role).toBe("user");
    expect(state.messages[0]!.text).toBe("Hello Claude");
    expect(state.status).toBe("thinking");
  });

  it("addTextDelta creates or appends to assistant message", () => {
    useClaudeStore.getState().addTextDelta("msg_1", "Hello");
    useClaudeStore.getState().addTextDelta("msg_1", " world");
    const state = useClaudeStore.getState();
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0]!.role).toBe("assistant");
    expect(state.messages[0]!.text).toBe("Hello world");
    expect(state.messages[0]!.isStreaming).toBe(true);
  });

  it("startTool adds to activeTools", () => {
    useClaudeStore.getState().startTool("tu_1", "Read", "Reading proposal.md");
    const state = useClaudeStore.getState();
    expect(state.activeTools).toHaveLength(1);
    expect(state.activeTools[0]!.toolId).toBe("tu_1");
    expect(state.activeTools[0]!.status).toBe("running");
  });

  it("endTool updates tool status", () => {
    useClaudeStore.getState().startTool("tu_1", "Read", "Reading proposal.md");
    useClaudeStore.getState().endTool("tu_1", true, "Completed");
    const state = useClaudeStore.getState();
    expect(state.activeTools[0]!.status).toBe("success");
  });

  it("completeTurn transitions back to connected", () => {
    useClaudeStore.getState().setSessionReady("sess-1", [], "model");
    useClaudeStore.getState().addUserMessage("test");
    useClaudeStore.getState().completeTurn(0.05, 1500);
    const state = useClaudeStore.getState();
    expect(state.status).toBe("connected");
    expect(state.totalCostUsd).toBe(0.05);
  });

  it("setError transitions to error status", () => {
    useClaudeStore.getState().setError("Network failed");
    const state = useClaudeStore.getState();
    expect(state.status).toBe("error");
    expect(state.error).toBe("Network failed");
  });

  it("reset clears everything", () => {
    useClaudeStore.getState().setSessionReady("sess-1", [], "model");
    useClaudeStore.getState().addUserMessage("test");
    useClaudeStore.getState().reset();
    const state = useClaudeStore.getState();
    expect(state.status).toBe("disconnected");
    expect(state.messages).toEqual([]);
    expect(state.sessionId).toBeNull();
  });
});
