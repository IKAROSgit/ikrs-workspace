import { describe, it, expect } from "vitest";
import {
  SKILL_DOMAINS,
  isSkillDomain,
  type SkillUpdateStatus,
  type SkillFolderInfo,
} from "@/types/skills";

describe("SKILL_DOMAINS", () => {
  it("contains exactly 8 domains", () => {
    expect(SKILL_DOMAINS).toHaveLength(8);
  });

  it("contains all expected domains", () => {
    const expected = [
      "communications",
      "planning",
      "creative",
      "operations",
      "legal",
      "finance",
      "research",
      "talent",
    ];
    expect([...SKILL_DOMAINS]).toEqual(expected);
  });
});

describe("isSkillDomain", () => {
  it("returns true for valid domains", () => {
    for (const domain of SKILL_DOMAINS) {
      expect(isSkillDomain(domain)).toBe(true);
    }
  });

  it("returns false for invalid strings", () => {
    expect(isSkillDomain("hospitality")).toBe(false);
    expect(isSkillDomain("")).toBe(false);
    expect(isSkillDomain("COMMUNICATIONS")).toBe(false);
  });
});

describe("SkillUpdateStatus type", () => {
  it("accepts valid status objects", () => {
    const status: SkillUpdateStatus = {
      updates_available: true,
      bundled_version: "1.1.0",
      installed_version: "1.0.0",
      updatable_folders: ["communications", "planning"],
      customized_folders: ["legal"],
      user_marked_custom: [],
    };
    expect(status.updates_available).toBe(true);
    expect(status.updatable_folders).toHaveLength(2);
  });
});

describe("SkillFolderInfo type", () => {
  it("accepts valid folder info", () => {
    const info: SkillFolderInfo = {
      domain: "communications",
      exists: true,
      hasClaudeMd: true,
      isCustomized: false,
      isUpdatable: true,
    };
    expect(info.domain).toBe("communications");
    expect(info.isCustomized).toBe(false);
  });
});
