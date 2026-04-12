export type ConsultantRole = "consultant" | "lead" | "admin";
export type EngagementStatus = "active" | "paused" | "completed" | "archived";
export type CredentialStatus = "active" | "expired" | "revoked";
export type TaskStatus = "todo" | "in_progress" | "done";
export type TaskPriority = "p1" | "p2" | "p3";
export type TaskSource = "manual" | "imported";
export type VaultStatus = "active" | "archived" | "deleted";
export type Theme = "dark" | "light";
export type McpServerType = "gmail" | "calendar" | "drive" | "obsidian";
export type McpHealthStatus = "healthy" | "reconnecting" | "down" | "stopped";

export interface Consultant {
  _v: 1;
  id: string;
  email: string;
  name: string;
  role: ConsultantRole;
  preferences: {
    theme: Theme;
    terminal: string;
    timezone: string;
  };
  createdAt: Date;
  updatedAt: Date;
}

export interface Client {
  _v: 1;
  id: string;
  name: string;
  domain: string;
  slug: string;
  branding: {
    logo?: string;
    primaryColor?: string;
    secondaryColor?: string;
  };
  createdAt: Date;
  updatedAt: Date;
}

export interface Engagement {
  _v: 1;
  id: string;
  consultantId: string;
  clientId: string;
  status: EngagementStatus;
  startDate: Date;
  endDate?: Date;
  settings: {
    timezone: string;
    billingRate?: number;
    description?: string;
    strictMcp?: boolean;
  };
  vault: {
    path: string;
    archivePath?: string;
    status: VaultStatus;
  };
  createdAt: Date;
  updatedAt: Date;
}

export interface Credential {
  _v: 1;
  id: string;
  engagementId: string;
  provider: "google";
  accountEmail: string;
  scopes: string[];
  keychainKey: string;
  lastRefreshed: Date;
  status: CredentialStatus;
  createdAt: Date;
}

export interface Subtask {
  title: string;
  done: boolean;
}

export interface Task {
  _v: 1;
  id: string;
  engagementId: string;
  title: string;
  description?: string;
  status: TaskStatus;
  priority: TaskPriority;
  dueDate?: Date;
  tags: string[];
  subtasks: Subtask[];
  sortOrder: number;
  source: TaskSource;
  createdAt: Date;
  updatedAt: Date;
}

export interface McpHealth {
  type: McpServerType;
  status: McpHealthStatus;
  pid?: number;
  lastPing?: Date;
  restartCount: number;
}

export type { ChatMessage, ToolActivity, ClaudeSessionStatus, AuthStatus, VersionCheck } from "./claude";

export type {
  SkillUpdateStatus,
  SkillDomain,
  SkillFolderInfo,
  ScaffoldSkillsParams,
  SkillUpdateParams,
} from "./skills";
export { SKILL_DOMAINS, isSkillDomain } from "./skills";
