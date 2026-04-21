export type ConsultantRole = "consultant" | "lead" | "admin";
export type EngagementStatus = "active" | "paused" | "completed" | "archived";
export type CredentialStatus = "active" | "expired" | "revoked";
// 2026-04-21 widened from "todo | in_progress | done" to the full 6-column
// Kanban taxonomy per Moe's product decision + Codex architecture review.
// Legacy migration: read-time adapter in EngagementProvider maps
// old "todo" → "backlog" and leaves "in_progress" / "done" untouched.
export type TaskStatus =
  | "backlog"
  | "in_progress"
  | "awaiting_client"
  | "blocked"
  | "in_review"
  | "done";
export type TaskPriority = "p1" | "p2" | "p3";
export type TaskSource = "manual" | "imported" | "claude";
export type TaskAssignee = "consultant" | "claude" | "client";
export type ClientVisibilityDefault = "open-book" | "private";
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
    /** Per-engagement default for a new task's `clientVisible` flag.
     *  Undefined resolves to "open-book" (Moe 2026-04-21 decision).
     *  Per-card `clientVisible` overrides this default. */
    defaultClientVisibility?: ClientVisibilityDefault;
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
  /** Schema version. Bumped from 1 → 2 alongside the status widening
   *  and visibility fields. Read-time migration handles v1 docs. */
  _v: 1 | 2;
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
  /** Per-card override of the engagement's defaultClientVisibility.
   *  When undefined, inherits from engagement.settings.
   *  When explicit true/false, wins. */
  clientVisible?: boolean;
  /** Set whenever clientVisible flips to true; cleared when it flips
   *  back to false. Used in the client portal "shared since" tooltip
   *  and in audit reconstruction. */
  sharedAt?: Date;
  /** Who owns this card logically. Used for avatar/icon rendering
   *  on the card face + for the audit trail. */
  assignee?: TaskAssignee;
  /** Vault path relative to the engagement root (e.g.
   *  "02-tasks/abc123.md") — the authoritative markdown Claude
   *  reads/writes. Absent on cards created purely in the UI until
   *  the vault-watch bridge materialises them. */
  vaultPath?: string;
  /** Drive URLs linked to this task. Phase 1 = user pastes the URL.
   *  Phase 2 = auto-ACL with drive scope bump. */
  driveLinks?: string[];
  /** Denormalised count of taskNotes/{id} docs whose taskId === this.
   *  Kept loosely in sync by the note composer; source of truth is
   *  the query on taskNotes. */
  notesCount?: number;
  createdAt: Date;
  updatedAt: Date;
}

/** Firestore `taskNotes/{id}` — one doc per note, not embedded.
 *  See Codex 2026-04-21 review §B.4 for why separate collection. */
export interface TaskNote {
  _v: 1;
  id: string;
  taskId: string;
  engagementId: string;
  authorKind: "consultant" | "claude" | "client";
  authorId: string;
  body: string;
  clientVisible: boolean;
  createdAt: Date;
  updatedAt: Date;
}

/** Firestore `ikrs_tasks/{taskId}/shareEvents/{eventId}` — append-only
 *  audit trail of visibility / critical-status changes. Firestore
 *  rules: `allow create: if engagement-member; update/delete: false`. */
export interface TaskShareEvent {
  _v: 1;
  id: string;
  taskId: string;
  engagementId: string;
  by: "consultant" | "claude";
  byId: string;
  field: "clientVisible" | "status";
  from: string | boolean | null;
  to: string | boolean | null;
  reason?: string;
  timestamp: Date;
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
