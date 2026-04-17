import { describe, expect, it } from "vitest";
import { isNewerVersion, _internals } from "@/lib/version-compare";

describe("isNewerVersion", () => {
  describe("basic ordering", () => {
    it("returns true when candidate is strictly newer (patch)", () => {
      expect(isNewerVersion("0.1.1", "0.1.0")).toBe(true);
    });
    it("returns true when candidate is strictly newer (minor)", () => {
      expect(isNewerVersion("0.2.0", "0.1.99")).toBe(true);
    });
    it("returns true when candidate is strictly newer (major)", () => {
      expect(isNewerVersion("1.0.0", "0.99.99")).toBe(true);
    });
    it("returns false for equal versions", () => {
      expect(isNewerVersion("1.2.3", "1.2.3")).toBe(false);
    });
    it("returns false when candidate is older (patch)", () => {
      expect(isNewerVersion("1.2.2", "1.2.3")).toBe(false);
    });
    it("returns false when candidate is older (minor)", () => {
      expect(isNewerVersion("1.1.99", "1.2.0")).toBe(false);
    });
    it("returns false when candidate is older (major)", () => {
      expect(isNewerVersion("0.99.99", "1.0.0")).toBe(false);
    });
  });

  describe("v-prefix handling", () => {
    it("strips lowercase v prefix on candidate", () => {
      expect(isNewerVersion("v1.2.3", "1.2.2")).toBe(true);
    });
    it("strips lowercase v prefix on current", () => {
      expect(isNewerVersion("1.2.3", "v1.2.2")).toBe(true);
    });
    it("strips uppercase V prefix", () => {
      expect(isNewerVersion("V2.0.0", "V1.99.99")).toBe(true);
    });
    it("treats v1.2.3 and 1.2.3 as equal", () => {
      expect(isNewerVersion("v1.2.3", "1.2.3")).toBe(false);
    });
  });

  describe("pre-release suffix handling", () => {
    // Auto-updater policy: pre-release suffixes are ignored for the
    // "is newer" test. `1.2.3-hotfix.1` does not upgrade over `1.2.3`.
    it("treats 1.2.3-hotfix.1 as equal to 1.2.3 (not newer)", () => {
      expect(isNewerVersion("1.2.3-hotfix.1", "1.2.3")).toBe(false);
    });
    it("still computes ordering correctly with suffixes on both sides", () => {
      expect(isNewerVersion("1.3.0-rc.1", "1.2.9-rc.5")).toBe(true);
    });
    it("a stable release is not newer than its own pre-release (both strip to same triplet)", () => {
      expect(isNewerVersion("1.0.0", "1.0.0-rc.1")).toBe(false);
    });
  });

  describe("missing segments", () => {
    it("treats missing patch as 0 (candidate side)", () => {
      expect(isNewerVersion("1.2", "1.1.99")).toBe(true);
    });
    it("treats missing patch as 0 (current side)", () => {
      expect(isNewerVersion("1.2.1", "1.2")).toBe(true);
    });
    it("treats missing minor+patch as 0", () => {
      expect(isNewerVersion("2", "1.99.99")).toBe(true);
    });
    it("1.0.0 and 1 compare as equal", () => {
      expect(isNewerVersion("1", "1.0.0")).toBe(false);
    });
  });

  describe("malformed input — defence against MITM substitution", () => {
    it("returns false on empty candidate", () => {
      expect(isNewerVersion("", "1.0.0")).toBe(false);
    });
    it("returns false on empty current", () => {
      expect(isNewerVersion("1.0.0", "")).toBe(false);
    });
    it("returns false on non-numeric candidate", () => {
      expect(isNewerVersion("banana", "1.0.0")).toBe(false);
    });
    it("returns false on mixed letters+numbers", () => {
      expect(isNewerVersion("1.2a.3", "1.0.0")).toBe(false);
    });
    it("returns false on doubled separator (1..2)", () => {
      expect(isNewerVersion("1..2", "1.0.0")).toBe(false);
    });
    it("returns false on too many segments (1.2.3.4)", () => {
      expect(isNewerVersion("1.2.3.4", "1.0.0")).toBe(false);
    });
    it("returns false on negative numbers", () => {
      // Regex rejects `-` so the whole version fails to parse (the
      // leading minus looks like a pre-release suffix which the parser
      // doesn't strip on the major side).
      expect(isNewerVersion("-1.0.0", "1.0.0")).toBe(false);
    });
    it("returns false when candidate is null-ish", () => {
      // @ts-expect-error deliberate bad input
      expect(isNewerVersion(null, "1.0.0")).toBe(false);
      // @ts-expect-error deliberate bad input
      expect(isNewerVersion(undefined, "1.0.0")).toBe(false);
    });
  });

  describe("attack scenarios", () => {
    // Scenarios modelled on the Codex Phase 4b retroactive review's
    // supply-chain concerns: an attacker replays a real, validly-signed
    // old `latest.json` that points at a real, validly-signed old
    // bundle. Our Layer 2 check is what rejects the downgrade.
    it("rejects replay of an older signed manifest pointing to older signed bundle", () => {
      // App is running v0.3.0. Attacker serves a signed manifest
      // claiming v0.2.0. signature verifies, version is older.
      expect(isNewerVersion("0.2.0", "0.3.0")).toBe(false);
    });
    it("rejects equal-version replay (still a downgrade-adjacent risk via re-install)", () => {
      expect(isNewerVersion("0.3.0", "0.3.0")).toBe(false);
    });
    it("rejects manifest with crafted pre-release claiming to be newer", () => {
      // Attacker hopes the naïve string compare would put
      // "1.0.0-hotfix.999" ahead of "1.0.0". Our implementation
      // strips the suffix before comparing.
      expect(isNewerVersion("1.0.0-hotfix.999", "1.0.0")).toBe(false);
    });
  });
});

describe("_internals.parseVersion", () => {
  it("parses standard triplet", () => {
    expect(_internals.parseVersion("1.2.3")).toEqual({ major: 1, minor: 2, patch: 3 });
  });
  it("returns null on malformed input", () => {
    expect(_internals.parseVersion("not a version")).toBeNull();
    expect(_internals.parseVersion("")).toBeNull();
  });
});
