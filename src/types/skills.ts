/** Status of skill updates for an engagement. Mirrors Rust SkillUpdateStatus. */
export interface SkillUpdateStatus {
  updates_available: boolean;
  bundled_version: string;
  installed_version: string;
  updatable_folders: string[];
  customized_folders: string[];
  user_marked_custom: string[];
}

/** The 8 bundled skill domains. */
export const SKILL_DOMAINS = [
  "communications",
  "planning",
  "creative",
  "operations",
  "legal",
  "finance",
  "research",
  "talent",
] as const;

export type SkillDomain = (typeof SKILL_DOMAINS)[number];

/** Info about a single skill folder in an engagement. */
export interface SkillFolderInfo {
  domain: SkillDomain;
  exists: boolean;
  hasClaudeMd: boolean;
  isCustomized: boolean;
  isUpdatable: boolean;
}

/** Parameters for scaffolding engagement skills. */
export interface ScaffoldSkillsParams {
  engagementPath: string;
  clientName: string;
  clientSlug: string;
  engagementTitle: string;
  engagementDescription: string;
  consultantName: string;
  consultantEmail: string;
  timezone: string;
}

/** Parameters for checking/applying skill updates (adds startDate). */
export interface SkillUpdateParams extends ScaffoldSkillsParams {
  startDate: string;
}

/** Type guard: checks if a string is a valid SkillDomain. */
export function isSkillDomain(value: string): value is SkillDomain {
  return (SKILL_DOMAINS as readonly string[]).includes(value);
}
