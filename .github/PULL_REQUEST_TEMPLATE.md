<!-- PR template — required reading before merge. -->

## Summary

<!-- 1-3 bullets: what does this PR do, why now? -->

## Tier of change

- [ ] Trivial (typo, comment, refactor with no behavioural change)
- [ ] Code change (existing feature)
- [ ] Architecture (new service, file move, dep change, auth/identity flow)
- [ ] Secret handling (env var, keychain, token, credential)
- [ ] Firestore (new collection, field, rule, index)
- [ ] Scheduling (systemd, tokio interval, cron)
- [ ] Operator runbook (install/deploy/rotate)
- [ ] Phase advance (status moved between shipped/in-progress/deferred)

## Required for non-trivial PRs

- [ ] **`docs/ECOSYSTEM.md` updated** in this PR. The entries below the
      "Last verified" line reflect this change. (CI will fail without
      this for sensitive-file changes — see
      `scripts/check-ecosystem-docs.sh`.)
- [ ] Tests pass locally on the layer(s) touched:
  - [ ] Python (`cd heartbeat && .venv/bin/ruff check src tests && .venv/bin/mypy src && .venv/bin/pytest -ra`)
  - [ ] Rust (`cd src-tauri && cargo clippy --no-deps && cargo test`)
  - [ ] TypeScript (`npx tsc --noEmit && npm test`)
- [ ] No secrets committed (`git diff --cached` reviewed)
- [ ] Author email is `IKAROSgit@users.noreply.github.com` (Firebase
      email privacy is on for the operator's GitHub account)

## For architecture / secret / Firestore PRs (extra)

- [ ] Spec doc exists at `docs/specs/m{X}-phase-{Y}-{slug}.md` and is
      referenced in this PR description
- [ ] Pre-code adversarial challenge agent dispatched and findings
      addressed
- [ ] Post-code adversarial challenge agent dispatched (or scheduled
      before merge) and BLOCK findings fixed

## Test plan

<!-- Bulleted checklist for the human reviewer / soak tester -->

- [ ]
- [ ]

🤖 Generated with [Claude Code](https://claude.com/claude-code)
