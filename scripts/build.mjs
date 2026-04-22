#!/usr/bin/env node
// Build wrapper so any `--mode <name>` / `--mode=<name>` flag
// reaches BOTH the prebuild env-check AND `vite build`.
//
// Codex 2026-04-22 P2: previously `npm run build` chained
// `node scripts/check-env.mjs && tsc && vite build` — `npm run
// build -- --mode staging` tacked the arg on the END, so
// check-env validated `production` while vite built `staging`.
// They loaded different `.env.<mode>*` files. False pass or
// false fail.
//
// This wrapper parses --mode once, forwards it deterministically
// to check-env (via argv) and vite (via argv), and does tsc in
// between. Single source of truth.

import { spawn } from "node:child_process";

function resolveMode(argv) {
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--mode") return argv[i + 1] ?? null;
    if (argv[i]?.startsWith?.("--mode=")) {
      return argv[i].slice("--mode=".length);
    }
  }
  return null;
}

function run(cmd, args, { label, env } = {}) {
  // shell: true is required on Windows so `.cmd` shims (npx, npm)
  // are resolved correctly. Safe here because we never interpolate
  // user input into the command — cmd/args are static literals.
  // Codex 2026-04-22 P2 fix.
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, {
      stdio: "inherit",
      shell: process.platform === "win32",
      env: { ...process.env, ...(env ?? {}) },
    });
    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) return resolve();
      reject(new Error(`${label ?? cmd} exited with code ${code}`));
    });
  });
}

(async () => {
  // Forward ALL trailing CLI flags verbatim to vite (not just
  // --mode). Codex 2026-04-22: dropping non-mode flags would
  // regress existing usage like `npm run build -- --sourcemap` or
  // `--base=/foo`. For the env-check, we only need the mode, so
  // we extract that specifically and inject it without disturbing
  // the rest of the argv.
  const argv = process.argv.slice(2);
  const mode = resolveMode(argv);
  const modeArgs = mode ? ["--mode", mode] : [];

  // 1. Prebuild env-var guard. Needs the mode (to validate the
  //    right .env.<mode>* files). Other flags are irrelevant here.
  await run("node", ["scripts/check-env.mjs", ...modeArgs], {
    label: "check-env",
  });

  // 2. TypeScript typecheck. Doesn't care about args.
  await run("npx", ["tsc", "--noEmit"], { label: "tsc" });

  // 3. Vite build with the FULL argv forwarded — --mode,
  //    --sourcemap, --base, --emptyOutDir, and anything else vite
  //    accepts.
  await run("npx", ["vite", "build", ...argv], { label: "vite build" });
})().catch((e) => {
  console.error(`[build] ${e.message}`);
  process.exit(1);
});
