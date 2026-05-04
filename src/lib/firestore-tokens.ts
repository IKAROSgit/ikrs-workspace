/**
 * Phase F — Encrypt OAuth tokens and sync to Firestore.
 *
 * After a successful per-engagement Google OAuth, the token payload
 * (already stored in the Mac keychain by redirect_server.rs) is
 * encrypted with AES-256-GCM using an operator-supplied key and
 * written to engagements/{eid}/google_tokens/google in Firestore.
 *
 * The heartbeat (Admin SDK) reads + decrypts per-engagement.
 * Refresh-token rotation writes back encrypted.
 *
 * Encryption uses the WebCrypto API (available in Tauri's webview).
 * The ciphertext field contains ciphertext || 16-byte GCM auth tag
 * (WebCrypto's default output format).
 */

import { doc, getDoc, setDoc, serverTimestamp, updateDoc } from "firebase/firestore";
import { db } from "./firebase";

const KEY_ENV = import.meta.env.VITE_TOKEN_ENCRYPTION_KEY ?? "";
const KEY_VERSION = Number(import.meta.env.VITE_TOKEN_ENCRYPTION_KEY_VERSION ?? "1");

/**
 * Decode the base64-encoded 32-byte AES key from the env var.
 * Returns null if the key is missing or malformed.
 */
function getEncryptionKey(): Uint8Array | null {
  if (!KEY_ENV) return null;
  try {
    const raw = Uint8Array.from(atob(KEY_ENV), (c) => c.charCodeAt(0));
    if (raw.length !== 32) return null;
    return raw;
  } catch {
    return null;
  }
}

/**
 * Encrypt a plaintext string with AES-256-GCM.
 * Returns { ciphertext, iv } both as base64 strings.
 * ciphertext includes the 16-byte GCM auth tag appended by WebCrypto.
 */
async function encrypt(
  plaintext: string,
  keyBytes: Uint8Array,
): Promise<{ ciphertext: string; iv: string }> {
  const key = await crypto.subtle.importKey(
    "raw",
    keyBytes,
    { name: "AES-GCM" },
    false,
    ["encrypt"],
  );

  // 12-byte random IV — never reuse with the same key.
  const iv = crypto.getRandomValues(new Uint8Array(12));

  const encoded = new TextEncoder().encode(plaintext);
  const encrypted = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv },
    key,
    encoded,
  );

  return {
    ciphertext: uint8ToBase64(new Uint8Array(encrypted)),
    iv: uint8ToBase64(iv),
  };
}

function uint8ToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary);
}

/**
 * Encrypt the token payload and write it to Firestore.
 *
 * Called from SettingsView / ChatView after oauth:token-stored fires.
 * The token payload is read from the keychain via getCredential().
 *
 * THROWS if the encryption key is missing. The caller should surface
 * this to the user — a missing key means Firestore sync is broken
 * and the heartbeat will never see the new token.
 */
export async function syncTokenToFirestore(
  engagementId: string,
  tokenPayloadJson: string,
): Promise<void> {
  const keyBytes = getEncryptionKey();
  if (!keyBytes) {
    throw new Error(
      "VITE_TOKEN_ENCRYPTION_KEY is missing or malformed in .env.local. " +
      "OAuth succeeded but the token was NOT synced to Firestore — " +
      "the heartbeat will not see this token. " +
      "Add the encryption key from the VM's /etc/ikrs-heartbeat/secrets.env " +
      "to .env.local and rebuild. See heartbeat/README.md.",
    );
  }

  const { ciphertext, iv } = await encrypt(tokenPayloadJson, keyBytes);

  // Fetch the connected email before writing — we'll store it as
  // an unencrypted field for UI display (not sensitive; just the
  // email address, not the token).
  let connectedEmail: string | null = null;
  try {
    const payload = JSON.parse(tokenPayloadJson);
    const resp = await fetch(
      "https://www.googleapis.com/oauth2/v3/userinfo",
      { headers: { Authorization: `Bearer ${payload.access_token}` } },
    );
    if (resp.ok) {
      const info = await resp.json();
      connectedEmail = info.email ?? null;
    }
  } catch {
    // Non-fatal — email will show as "unknown" in UI
  }

  const ref = doc(db, "engagements", engagementId, "google_tokens", "google");
  await setDoc(ref, {
    ciphertext,
    iv,
    keyVersion: KEY_VERSION,
    updatedAt: serverTimestamp(),
    writtenBy: "tauri",
    ...(connectedEmail ? { connectedEmail } : {}),
  });
}

/**
 * Read the cached connectedEmail from the google_tokens doc.
 * Returns null if not set or doc doesn't exist.
 */
export async function getConnectedEmail(
  engagementId: string,
): Promise<string | null> {
  const ref = doc(db, "engagements", engagementId, "google_tokens", "google");
  const snap = await getDoc(ref);
  if (!snap.exists()) return null;
  return (snap.data()?.connectedEmail as string) ?? null;
}

/**
 * Refresh the connectedEmail by calling userinfo with the current
 * access token from keychain, then updating the Firestore doc.
 */
export async function refreshConnectedEmail(
  engagementId: string,
  accessToken: string,
): Promise<string | null> {
  try {
    const resp = await fetch(
      "https://www.googleapis.com/oauth2/v3/userinfo",
      { headers: { Authorization: `Bearer ${accessToken}` } },
    );
    if (!resp.ok) return null;
    const info = await resp.json();
    const email = info.email as string | undefined;
    if (!email) return null;

    const ref = doc(db, "engagements", engagementId, "google_tokens", "google");
    await updateDoc(ref, { connectedEmail: email });
    return email;
  } catch {
    return null;
  }
}
