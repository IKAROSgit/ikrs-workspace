import { describe, it, expect, beforeEach } from "vitest";
import { useEngagementStore } from "@/stores/engagementStore";

describe("engagementStore", () => {
  beforeEach(() => {
    useEngagementStore.setState({
      engagements: [],
      activeEngagementId: null,
      clients: [],
    });
  });

  it("starts with empty state", () => {
    const state = useEngagementStore.getState();
    expect(state.engagements).toEqual([]);
    expect(state.activeEngagementId).toBeNull();
  });

  it("sets active engagement", () => {
    useEngagementStore.getState().setActiveEngagement("eng-1");
    expect(useEngagementStore.getState().activeEngagementId).toBe("eng-1");
  });

  it("adds engagement", () => {
    const eng = {
      _v: 1 as const,
      id: "eng-1",
      consultantId: "uid-1",
      clientId: "client-1",
      status: "active" as const,
      startDate: new Date(),
      settings: { timezone: "Asia/Dubai" },
      vault: { path: "/tmp", status: "active" as const },
      createdAt: new Date(),
      updatedAt: new Date(),
    };
    useEngagementStore.getState().addEngagement(eng);
    expect(useEngagementStore.getState().engagements).toHaveLength(1);
  });

  it("derives active engagement via selector", () => {
    const eng = {
      _v: 1 as const,
      id: "eng-1",
      consultantId: "uid-1",
      clientId: "client-1",
      status: "active" as const,
      startDate: new Date(),
      settings: { timezone: "Asia/Dubai" },
      vault: { path: "/tmp", status: "active" as const },
      createdAt: new Date(),
      updatedAt: new Date(),
    };
    useEngagementStore.getState().addEngagement(eng);
    useEngagementStore.getState().setActiveEngagement("eng-1");
    // CODEX MUST-FIX #2: derive via selector, not getter
    const state = useEngagementStore.getState();
    const active = state.engagements.find((e) => e.id === state.activeEngagementId);
    expect(active?.id).toBe("eng-1");
  });
});
