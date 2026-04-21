#!/usr/bin/env node
// Prebuild env-var guard.
//
// Runs before `vite build` (wired from package.json). If any critical
// VITE_* variable is missing OR still equal to a known placeholder
// value, the build fails loudly instead of producing a binary that
// compiles cleanly but has non-functional auth / Firebase.
//
// Added 2026-04-22 after a regression where `pnpm tauri build` on a
// host with an empty .env.local produced a working binary whose
// "Sign in with Google" button appeared to do nothing — actually
// it was firing against empty client IDs. Not the cause of that
// session's specific bug (dual-stack regression) but a real class
// of future bug this guard prevents.
//
// Skipped in CI (`CI=true`), since CI injects placeholder values
// during typecheck/build validation and doesn't produce a
// consumer-installable binary.

import { loadEnv } from "vite";

const REQUIRED = [
  // Google OAuth — sign-in to the app + per-engagement Google access.
  "VITE_GOOGLE_OAUTH_CLIENT_ID",
  "VITE_GOOGLE_OAUTH_CLIENT_SECRET",
  // Firebase — auth provider + Firestore backend.
  "VITE_FIREBASE_API_KEY",
  "VITE_FIREBASE_AUTH_DOMAIN",
  "VITE_FIREBASE_PROJECT_ID",
  "VITE_FIREBASE_STORAGE_BUCKET",
  "VITE_FIREBASE_MESSAGING_SENDER_ID",
  "VITE_FIREBASE_APP_ID",
];

// Values that compile but produce a broken app. If a .env.local
// contains these, we treat them as "not set" for the purposes of
// this check.
const KNOWN_PLACEHOLDERS = new Set([
  "ci-placeholder",
  "GENERATED_PUBLIC_KEY_HERE",
  "your-api-key",
  "your-project-id",
  "0000000000",
  "",
]);

if (process.env.CI === "true") {
  console.log("[check-env] CI=true — skipping env check.");
  process.exit(0);
}

// Use Vite's own loadEnv so we honour the exact file-precedence
// order vite build does at runtime:
//   .env  →  .env.[mode]  →  .env.local  →  .env.[mode].local
// Anything on process.env wins over all of them (standard vite
// behaviour). Codex 2026-04-22 pre-push P2: a hand-rolled .env.local
// loader would false-fail valid builds that keep VITE_* in .env
// (shared defaults) or .env.production (prod-only).
//
// Mode resolution (Codex P2 follow-up): `vite build` defaults to
// `production` REGARDLESS of `NODE_ENV` unless `--mode` is passed.
// If we derived mode from NODE_ENV here, a shell with
// `NODE_ENV=development` would make this script read
// `.env.development*` while vite build still reads
// `.env.production*` — the guard validates a different env set
// than the build actually uses. Match vite's default instead.
// Pass `--mode <name>` or env `MODE=<name>` to override for
// non-default build modes.
function resolveBuildMode() {
  const argIdx = process.argv.indexOf("--mode");
  if (argIdx !== -1 && process.argv[argIdx + 1]) {
    return process.argv[argIdx + 1];
  }
  if (process.env.MODE) return process.env.MODE;
  return "production";
}
const mode = resolveBuildMode();
const loaded = loadEnv(mode, process.cwd(), "VITE_");
// Merge loaded over process.env fallback (but process.env wins so
// a caller-exported env always overrides).
const resolved = { ...loaded, ...process.env };

const missing = [];
const placeholder = [];
for (const key of REQUIRED) {
  const val = resolved[key];
  if (val === undefined || val === "") {
    missing.push(key);
  } else if (KNOWN_PLACEHOLDERS.has(val.trim())) {
    placeholder.push(key);
  }
}

if (missing.length === 0 && placeholder.length === 0) {
  console.log(`[check-env] all ${REQUIRED.length} required VITE_* vars present.`);
  process.exit(0);
}

console.error("[check-env] ERROR — build blocked.\n");
if (missing.length) {
  console.error("Missing variables:");
  for (const k of missing) console.error(`  - ${k}`);
  console.error("");
}
if (placeholder.length) {
  console.error("Variables still set to a known placeholder:");
  for (const k of placeholder) console.error(`  - ${k}`);
  console.error("");
}
console.error("Fix:");
console.error("  1. Create/edit ikrs-workspace/.env.local with the real values.");
console.error("  2. Re-run `pnpm tauri build` (or `npm run build`).");
console.error("");
console.error(
  "For CI or automated builds, export CI=true to bypass this check.",
);
process.exit(1);
