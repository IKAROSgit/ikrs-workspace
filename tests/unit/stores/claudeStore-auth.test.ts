import { describe, it, expect, beforeEach } from "vitest";
import { useClaudeStore } from "@/stores/claudeStore";

describe("claudeStore auth-error state", () => {
  beforeEach(() => {
    useClaudeStore.getState().reset();
  });

  it("setAuthError stores server and hint", () => {
    useClaudeStore.getState().setAuthError("gmail", "Token expired");
    const { authError } = useClaudeStore.getState();
    expect(authError).toEqual({ server: "gmail", hint: "Token expired" });
  });

  it("clearAuthError resets to null", () => {
    useClaudeStore.getState().setAuthError("gmail", "Token expired");
    useClaudeStore.getState().clearAuthError();
    expect(useClaudeStore.getState().authError).toBeNull();
  });

  it("reset() clears authError", () => {
    useClaudeStore.getState().setAuthError("drive", "HTTP 401");
    useClaudeStore.getState().reset();
    expect(useClaudeStore.getState().authError).toBeNull();
  });

  it("setDisconnected clears session but preserves authError", () => {
    useClaudeStore.setState({ sessionId: "sess_1", status: "connected" });
    useClaudeStore.getState().setAuthError("gmail", "expired");
    useClaudeStore.getState().setDisconnected("process exited");
    expect(useClaudeStore.getState().sessionId).toBeNull();
    expect(useClaudeStore.getState().status).toBe("disconnected");
    expect(useClaudeStore.getState().authError).toEqual({ server: "gmail", hint: "expired" });
  });
});
