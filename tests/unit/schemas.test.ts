import { describe, it, expect } from "vitest";
import {
  ConsultantSchema,
  EngagementSchema,
  TaskSchema,
} from "@/lib/schemas";

describe("ConsultantSchema", () => {
  it("validates a valid consultant", () => {
    const result = ConsultantSchema.safeParse({
      _v: 1,
      id: "uid-123",
      email: "moe@ikaros.ae",
      name: "Moe Aqeel",
      role: "admin",
      preferences: { theme: "dark", terminal: "iTerm", timezone: "Asia/Dubai" },
      createdAt: new Date(),
      updatedAt: new Date(),
    });
    expect(result.success).toBe(true);
  });

  it("rejects invalid role", () => {
    const result = ConsultantSchema.safeParse({
      _v: 1,
      id: "uid-123",
      email: "moe@ikaros.ae",
      name: "Moe",
      role: "superadmin",
      preferences: { theme: "dark", terminal: "iTerm", timezone: "Asia/Dubai" },
      createdAt: new Date(),
      updatedAt: new Date(),
    });
    expect(result.success).toBe(false);
  });
});

describe("EngagementSchema", () => {
  it("validates active engagement", () => {
    const result = EngagementSchema.safeParse({
      _v: 1,
      id: "eng-1",
      consultantId: "uid-123",
      clientId: "client-1",
      status: "active",
      startDate: new Date(),
      settings: { timezone: "Asia/Dubai" },
      vault: {
        path: "~/.ikrs-workspace/vaults/blr-world/",
        status: "active",
      },
      createdAt: new Date(),
      updatedAt: new Date(),
    });
    expect(result.success).toBe(true);
  });

  it("rejects engagement without consultantId", () => {
    const result = EngagementSchema.safeParse({
      _v: 1,
      id: "eng-1",
      clientId: "client-1",
      status: "active",
      startDate: new Date(),
      settings: { timezone: "Asia/Dubai" },
      vault: { path: "/tmp", status: "active" },
      createdAt: new Date(),
      updatedAt: new Date(),
    });
    expect(result.success).toBe(false);
  });
});

describe("TaskSchema", () => {
  it("validates task with subtasks", () => {
    const result = TaskSchema.safeParse({
      _v: 1,
      id: "task-1",
      engagementId: "eng-1",
      title: "Review proposal",
      status: "todo",
      priority: "p1",
      tags: ["#drive"],
      subtasks: [{ title: "Download deck", done: false }],
      sortOrder: 0,
      source: "manual",
      createdAt: new Date(),
      updatedAt: new Date(),
    });
    expect(result.success).toBe(true);
  });
});
