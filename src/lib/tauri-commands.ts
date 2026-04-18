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
export interface OAuthFlowResult {
  auth_url: string;
  actual_port: number;
}

export async function startOAuthFlow(
  engagementId: string,
  clientId: string,
  clientSecret: string,
  redirectPort: number,
  scopes: string[],
): Promise<OAuthFlowResult> {
  return invoke("start_oauth_flow", {
    engagementId,
    clientId,
    clientSecret,
    redirectPort,
    scopes,
  });
}

export async function cancelOAuthFlow(): Promise<void> {
  return invoke("cancel_oauth_flow");
}

// Firebase identity flow (PKCE + OIDC via system browser).
// Separate from startOAuthFlow because it targets Firebase identity
// login, not per-engagement Google API access. See
// src-tauri/src/oauth/identity_server.rs for the design rationale.
//
// Google's Desktop-app OAuth clients require `client_secret` at the
// token endpoint even with PKCE (the secret is explicitly non-
// confidential for Desktop clients per Google's docs, but still
// required on the wire). Client ID + Secret both sourced from
// .env.local at Vite build time.
export async function startFirebaseIdentityFlow(
  clientId: string,
  clientSecret: string,
  redirectPort: number,
): Promise<OAuthFlowResult> {
  return invoke("start_firebase_identity_flow", {
    clientId,
    clientSecret,
    redirectPort,
  });
}

export async function cancelFirebaseIdentityFlow(): Promise<void> {
  return invoke("cancel_firebase_identity_flow");
}

// Clear the in-memory Google access-token cache. Called from logOut
// so the next consultant sign-in does not see the prior consultant's
// cached tokens.
export async function clearTokenCache(): Promise<void> {
  return invoke("clear_token_cache");
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
  strictMcp?: boolean,
): Promise<string> {
  return invoke("spawn_claude_session", {
    engagementId,
    engagementPath,
    resumeSessionId: resumeSessionId ?? null,
    clientSlug: clientSlug ?? null,
    strictMcp: strictMcp ?? null,
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
