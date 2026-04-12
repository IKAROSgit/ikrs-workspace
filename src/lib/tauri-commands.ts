import { invoke } from "@tauri-apps/api/core";
import type { AuthStatus, VersionCheck } from "@/types/claude";

export async function storeCredential(key: string, value: string): Promise<void> {
  return invoke("store_credential", { key, value });
}

export async function getCredential(key: string): Promise<string | null> {
  return invoke("get_credential", { key });
}

export async function deleteCredential(key: string): Promise<void> {
  return invoke("delete_credential", { key });
}

export function makeKeychainKey(engagementId: string, provider: string): string {
  return `ikrs:${engagementId}:${provider}`;
}

// OAuth
export interface OAuthStartResult {
  auth_url: string;
}

export interface TokenResponse {
  access_token: string;
  refresh_token: string | null;
  expires_in: number;
}

export async function startOAuth(
  clientId: string,
  redirectPort: number,
  scopes: string[],
): Promise<OAuthStartResult> {
  return invoke("start_oauth", { clientId, redirectPort, scopes });
}

export async function exchangeOAuthCode(
  code: string,
  clientId: string,
  redirectPort: number,
): Promise<TokenResponse> {
  return invoke("exchange_oauth_code", { code, clientId, redirectPort });
}

// Vault lifecycle
export async function createVault(clientSlug: string): Promise<string> {
  return invoke("create_vault", { clientSlug });
}

export async function archiveVault(clientSlug: string): Promise<string> {
  return invoke("archive_vault", { clientSlug });
}

export async function restoreVault(archivePath: string): Promise<string> {
  return invoke("restore_vault", { archivePath });
}

export async function deleteVault(clientSlug: string): Promise<void> {
  return invoke("delete_vault", { clientSlug });
}

// Claude M2 — Embedded Subprocess

export async function claudeVersionCheck(): Promise<VersionCheck> {
  return invoke("claude_version_check");
}

export async function claudeAuthStatus(): Promise<AuthStatus> {
  return invoke("claude_auth_status");
}

export async function claudeAuthLogin(): Promise<void> {
  return invoke("claude_auth_login");
}

export async function spawnClaudeSession(
  engagementId: string,
  engagementPath: string,
  resumeSessionId?: string,
  clientSlug?: string,
): Promise<string> {
  return invoke("spawn_claude_session", {
    engagementId,
    engagementPath,
    resumeSessionId: resumeSessionId ?? null,
    clientSlug: clientSlug ?? null,
  });
}

export async function sendClaudeMessage(
  sessionId: string,
  message: string,
): Promise<void> {
  return invoke("send_claude_message", { sessionId, message });
}

export async function killClaudeSession(sessionId: string): Promise<void> {
  return invoke("kill_claude_session", { sessionId });
}

export async function getResumeSessionId(
  engagementId: string,
): Promise<string | null> {
  return invoke("get_resume_session_id", { engagementId });
}

// Skills — Phase 2

import type {
  SkillUpdateStatus,
  ScaffoldSkillsParams,
  SkillUpdateParams,
} from "@/types/skills";

export async function scaffoldEngagementSkills(
  params: ScaffoldSkillsParams,
): Promise<string> {
  return invoke("scaffold_engagement_skills_cmd", {
    engagementPath: params.engagementPath,
    clientName: params.clientName,
    clientSlug: params.clientSlug,
    engagementTitle: params.engagementTitle,
    engagementDescription: params.engagementDescription,
    consultantName: params.consultantName,
    consultantEmail: params.consultantEmail,
    timezone: params.timezone,
  });
}

export async function checkSkillUpdates(
  params: SkillUpdateParams,
): Promise<SkillUpdateStatus> {
  return invoke("check_skill_updates_cmd", {
    engagementPath: params.engagementPath,
    clientName: params.clientName,
    clientSlug: params.clientSlug,
    engagementTitle: params.engagementTitle,
    engagementDescription: params.engagementDescription,
    consultantName: params.consultantName,
    consultantEmail: params.consultantEmail,
    timezone: params.timezone,
    startDate: params.startDate,
  });
}

export async function applySkillUpdates(
  params: SkillUpdateParams,
  foldersToUpdate: string[],
): Promise<void> {
  return invoke("apply_skill_updates_cmd", {
    engagementPath: params.engagementPath,
    foldersToUpdate,
    clientName: params.clientName,
    clientSlug: params.clientSlug,
    engagementTitle: params.engagementTitle,
    engagementDescription: params.engagementDescription,
    consultantName: params.consultantName,
    consultantEmail: params.consultantEmail,
    timezone: params.timezone,
    startDate: params.startDate,
  });
}
