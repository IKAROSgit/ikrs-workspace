/**
 * Minimal semver-style version comparison used by the auto-updater
 * Layer 2 downgrade protection in UpdateChecker.
 *
 * Why not npm `semver`? To avoid pulling another dependency into the
 * frontend bundle. Our version scheme is simple `MAJOR.MINOR.PATCH`
 * with optional `v` prefix and optional `-prerelease` suffix; Tauri's
 * plugin-updater already does full semver comparison before returning
 * an Update object, so this helper is the belt in a belt-and-braces
 * defence and only needs to be right about the "is strictly newer?"
 * question.
 *
 * Semantics:
 * - Strips a leading `v` / `V` if present.
 * - Ignores pre-release suffix (`-hotfix.1`, `-rc.2`, etc.) for the
 *   "is newer" question: `1.2.3` and `1.2.3-hotfix.1` compare as
 *   equal. This is stricter than full semver (where `1.2.3-foo <
 *   1.2.3`), and it is the correct posture for auto-update: we do
 *   not want pre-release builds to be installed on stable channels.
 * - Missing minor/patch defaults to 0 (`v1` == `v1.0.0`).
 * - Non-numeric components cause the parse to fail — `isNewerVersion`
 *   returns false in that case rather than throwing.
 */

interface ParsedVersion {
  major: number;
  minor: number;
  patch: number;
}

function parseVersion(input: string): ParsedVersion | null {
  if (typeof input !== "string" || input.length === 0) return null;
  const stripped = input.replace(/^v/i, "").split("-")[0] ?? "";
  if (stripped.length === 0) return null;
  const parts = stripped.split(".");
  if (parts.length === 0 || parts.length > 3) return null;

  const nums: number[] = [];
  for (const p of parts) {
    // Reject empty segments ("1..2") and non-numeric
    if (p.length === 0) return null;
    if (!/^\d+$/.test(p)) return null;
    const n = parseInt(p, 10);
    if (Number.isNaN(n)) return null;
    nums.push(n);
  }

  return {
    major: nums[0] ?? 0,
    minor: nums[1] ?? 0,
    patch: nums[2] ?? 0,
  };
}

/**
 * Returns true iff `candidate` is strictly greater than `current` in
 * major.minor.patch ordering. Returns false for equal, older, or
 * unparseable inputs.
 */
export function isNewerVersion(candidate: string, current: string): boolean {
  const c = parseVersion(candidate);
  const cur = parseVersion(current);
  if (c === null || cur === null) return false;

  if (c.major !== cur.major) return c.major > cur.major;
  if (c.minor !== cur.minor) return c.minor > cur.minor;
  return c.patch > cur.patch;
}

/**
 * Exposed for tests only. Not part of the public surface.
 * @internal
 */
export const _internals = { parseVersion };
