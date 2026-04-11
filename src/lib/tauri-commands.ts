import { invoke } from "@tauri-apps/api/core";

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
