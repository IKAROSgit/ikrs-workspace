import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useClaudeStore } from "@/stores/claudeStore";
import { useEngagementStore } from "@/stores/engagementStore";

// Mock tauri commands
vi.mock("@/lib/tauri-commands", () => ({
  killClaudeSession: vi.fn(),
  spawnClaudeSession: vi.fn(),
  getResumeSessionId: vi.fn(() => Promise.resolve(null)),
  claudeVersionCheck: vi.fn(() =>
    Promise.resolve({ installed: true, meets_minimum: true, version: "2.1.0" })
  ),
  claudeAuthStatus: vi.fn(() => Promise.resolve({ loggedIn: true })),
}));

// Import after mocks
import { useWorkspaceSession } from "@/hooks/useWorkspaceSession";
import { spawnClaudeSession } from "@/lib/tauri-commands";

describe("useWorkspaceSession offline guards", () => {
  let originalOnLine: boolean;

  beforeEach(() => {
    originalOnLine = navigator.onLine;
    useClaudeStore.getState().reset();

    // Set up a mock engagement
    useEngagementStore.setState({
      activeEngagementId: "eng-1",
      engagements: [
        {
          id: "eng-1",
          consultantId: "c-1",
          clientId: "cl-1",
          status: "active",
          startDate: new Date(),
          settings: { timezone: "Asia/Dubai" },
          vault: { path: "/tmp/vault", status: "active" },
        },
      ] as any,
      clients: [
        { id: "cl-1", name: "Test", domain: "test.com", slug: "test", branding: {} },
      ] as any,
    });
  });

  afterEach(() => {
    Object.defineProperty(navigator, "onLine", {
      value: originalOnLine,
      writable: true,
      configurable: true,
    });
    vi.clearAllMocks();
  });

  it("connect() sets error and returns early when offline", async () => {
    Object.defineProperty(navigator, "onLine", {
      value: false,
      writable: true,
      configurable: true,
    });

    const { result } = renderHook(() => useWorkspaceSession());

    await act(async () => {
      await result.current.connect();
    });

    const state = useClaudeStore.getState();
    expect(state.error).toBe(
      "Unable to reach Claude. Check your internet connection and try again."
    );
    expect(spawnClaudeSession).not.toHaveBeenCalled();
  });

  it("connect() proceeds normally when online", async () => {
    Object.defineProperty(navigator, "onLine", {
      value: true,
      writable: true,
      configurable: true,
    });

    const { result } = renderHook(() => useWorkspaceSession());

    await act(async () => {
      await result.current.connect();
    });

    // Should have proceeded past the offline guard to spawn
    expect(spawnClaudeSession).toHaveBeenCalled();
  });

  it("switchEngagement() sets error and returns early when offline", async () => {
    Object.defineProperty(navigator, "onLine", {
      value: false,
      writable: true,
      configurable: true,
    });

    const { result } = renderHook(() => useWorkspaceSession());

    await act(async () => {
      await result.current.switchEngagement("eng-1");
    });

    const state = useClaudeStore.getState();
    expect(state.error).toBe(
      "Unable to reach Claude. Check your internet connection and try again."
    );
  });
});
