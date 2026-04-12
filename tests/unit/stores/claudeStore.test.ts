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

  it("startTool stores toolInput", () => {
    useClaudeStore.getState().startTool("tu_1", "Read", "Reading file.md", '{"file_path":"/test.md"}');
    const state = useClaudeStore.getState();
    expect(state.activeTools[0]!.toolInput).toBe('{"file_path":"/test.md"}');
  });

  it("endTool stores resultContent", () => {
    useClaudeStore.getState().startTool("tu_1", "Read", "Reading file.md");
    useClaudeStore.getState().endTool("tu_1", true, "Completed", "file contents here");
    const state = useClaudeStore.getState();
    expect(state.activeTools[0]!.resultContent).toBe("file contents here");
  });

  it("setSessionReady sets sessionStartedAt", () => {
    const before = Date.now();
    useClaudeStore.getState().setSessionReady("sess-1", ["Read"], "claude-sonnet-4-6");
    const state = useClaudeStore.getState();
    expect(state.sessionStartedAt).toBeGreaterThanOrEqual(before);
    expect(state.sessionStartedAt).toBeLessThanOrEqual(Date.now());
  });

  describe("history partitioning", () => {
    it("saveAndClearHistory saves messages and clears state", () => {
      useClaudeStore.getState().addUserMessage("Hello");
      useClaudeStore.getState().addTextDelta("msg_1", "Hi back");
      useClaudeStore.getState().startTool("tu_1", "Read", "Reading file");

      useClaudeStore.getState().saveAndClearHistory("eng-1");
      const state = useClaudeStore.getState();
      expect(state.messages).toEqual([]);
      expect(state.activeTools).toEqual([]);
      expect(state.engagementId).toBeNull();
      expect(state.historyCache["eng-1"]).toHaveLength(2);
    });

    it("loadHistory restores messages", () => {
      useClaudeStore.getState().addUserMessage("Hello");
      useClaudeStore.getState().saveAndClearHistory("eng-1");

      // Switch to different engagement
      useClaudeStore.getState().addUserMessage("Other");
      useClaudeStore.getState().saveAndClearHistory("eng-2");

      // Load eng-1
      useClaudeStore.getState().loadHistory("eng-1");
      const state = useClaudeStore.getState();
      expect(state.engagementId).toBe("eng-1");
      expect(state.messages).toHaveLength(1);
      expect(state.messages[0]!.text).toBe("Hello");
    });

    it("loadHistory returns empty for unknown engagement", () => {
      useClaudeStore.getState().loadHistory("eng-unknown");
      const state = useClaudeStore.getState();
      expect(state.messages).toEqual([]);
      expect(state.engagementId).toBe("eng-unknown");
    });

    it("saveAndClearHistory applies FIFO cap of 50", () => {
      for (let i = 0; i < 60; i++) {
        useClaudeStore.getState().addUserMessage(`Message ${i}`);
      }
      useClaudeStore.getState().saveAndClearHistory("eng-full");
      const cached = useClaudeStore.getState().historyCache["eng-full"];
      expect(cached).toHaveLength(50);
      // Should keep the LAST 50 (messages 10-59)
      expect(cached![0]!.text).toBe("Message 10");
      expect(cached![49]!.text).toBe("Message 59");
    });
  });
});
