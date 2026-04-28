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

import { doc, setDoc, serverTimestamp } from "firebase/firestore";
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
 * Silently no-ops if the encryption key is not configured (operator
 * hasn't set up Phase F yet — backwards-compatible with Phase E).
 */
export async function syncTokenToFirestore(
  engagementId: string,
  tokenPayloadJson: string,
): Promise<void> {
  const keyBytes = getEncryptionKey();
  if (!keyBytes) {
    console.warn(
      "[firestore-tokens] VITE_TOKEN_ENCRYPTION_KEY not set; skipping Firestore token sync.",
    );
    return;
  }

  const { ciphertext, iv } = await encrypt(tokenPayloadJson, keyBytes);

  const ref = doc(db, "engagements", engagementId, "google_tokens", "google");
  await setDoc(ref, {
    ciphertext,
    iv,
    keyVersion: KEY_VERSION,
    updatedAt: serverTimestamp(),
    writtenBy: "tauri",
  });
}
