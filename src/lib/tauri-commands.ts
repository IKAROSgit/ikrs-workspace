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

// MCP Process Management
export type McpServerType = "gmail" | "calendar" | "drive" | "obsidian";

export interface McpStatusResult {
  server_type: McpServerType;
  status: "healthy" | "reconnecting" | "down" | "stopped";
  pid: number | null;
  restart_count: number;
}

interface SpawnMcpArgs {
  server_type: McpServerType;
  command: string;
  args: string[];
  env: Record<string, string>;
}

export async function spawnMcp(args: SpawnMcpArgs): Promise<number> {
  return invoke("spawn_mcp", { args });
}

export async function killMcp(serverType: McpServerType): Promise<void> {
  return invoke("kill_mcp", { serverType });
}

export async function killAllMcp(): Promise<void> {
  return invoke("kill_all_mcp");
}

export async function mcpHealth(
  serverType: McpServerType,
): Promise<McpStatusResult> {
  return invoke("mcp_health", { serverType });
}

export async function restartMcp(
  serverType: McpServerType,
): Promise<number> {
  return invoke("restart_mcp", { serverType });
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
): Promise<string> {
  return invoke("spawn_claude_session", { engagementId, engagementPath });
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
