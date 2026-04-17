# Hosting decision — `latest.json` for the auto-updater

**Status:** OPEN — awaits Moe's pick between option A and option B
**Spec:** `docs/specs/m2-phase4c-release-readiness-design.md` §2
**Blocker context:** `tauri.conf.json:27` currently points at `https://github.com/IKAROSgit/ikrs-workspace/releases/latest/download/latest.json`. The repo is currently **private**. Unauthenticated `curl` against a private repo's release download URL returns 404, so every installed app would fail its update check in the wild. One of the two options below must be chosen before `v0.1.0` ships, otherwise auto-update is fail-closed on day one.

---

## Option A — Make the repo public

### What this does

Flip `IKAROSgit/ikrs-workspace` visibility in repo Settings from **Private** to **Public**. No code or config change needed.

### Prerequisites

Before flipping:
1. **Secret scan the full commit history.** Recommended: `gitleaks detect --no-git -v`, then `gitleaks protect --staged`. Also run `trufflehog git file://.` for double coverage.
2. **Rotate any credential that shows up in the scan**, even if it was in an early commit and has since been removed — commits remain in history forever on a public repo.
3. **Audit `.env*` patterns, `credentials.json`, `.p12`, `.key`, `.pem`** — none of these should be tracked. Confirm `.gitignore` catches them.
4. **Check issue titles + comments** for customer names or sensitive project references.
5. **Review open PRs** — PR descriptions become public.

### Pros

- **Zero ongoing cost.** GitHub serves the `latest.json` and release artefacts for free.
- **Zero new infrastructure.** No extra DNS, no extra auth to maintain.
- **Standard pattern** used by Tauri-first projects (e.g. Tauri's own example repo, `tauri-apps/updater-action`).
- **Signals openness** — consultant market is used to inspecting the code of tools that will live on their machine. Removes a small adoption friction.
- Matches the Tauri updater plugin's default endpoint expectations exactly — no config change at all.

### Cons / risks

- **History becomes public forever.** If any stale secret is present in an old commit and missed by scanning, it's exposed. Rotation is the only remedy.
- **Issue + PR tracker is public.** Early product issues and discussions about clients become visible. Can be mitigated by opening Discussions separately or triaging internally before filing issues.
- **Some commit messages reference clients by name** (e.g. "BLR" in Phase 4a notes). These aren't secrets but are customer-relationship signals. A grep + history rewrite may be warranted if you want to obscure client names.
- **No fine-grained control** — you're fully public or fully private. Can't "release binaries to the world but keep source private." That's what option B gives you.

### Effort

- Prerequisites: 2-3 hours if the history is clean; a day if there are secrets to rotate.
- Actual flip: 30 seconds.

---

## Option B — Public CDN for `latest.json` + release binaries

### What this does

Keep the repo private. On every tag push, CI uploads `latest.json` + the signed release artefacts to a public bucket. Update the `tauri.conf.json:27` endpoint to the bucket URL.

Bucket candidates (pick one):

| Provider | Free tier | Ongoing cost at expected usage | Friction |
|----------|-----------|--------------------------------|----------|
| **Cloudflare R2** | 10 GB storage + 1M req/mo free | ~$0 until traffic is meaningful | Requires Cloudflare account + R2 API token in GH Secrets |
| **GCS** (since we already have `ikaros-portal` GCP project) | 5 GB free on always-free tier | < $1/mo at 100 consultants monthly | Low friction — SA auth via already-configured GH OIDC or service account key |
| **GitHub Packages (release assets only)** | Free for public, not for private | — | Doesn't solve the problem (same visibility as the repo) |

**Recommended:** GCS under `ikaros-portal` project, path `gs://ikrs-workspace-releases-public/latest.json` + `…/<tag>/<artefact>`. Existing IAM patterns apply.

### Pros

- **Private repo stays private.** Source, issue tracker, commit history remain internal.
- **Hotlinkable CDN** — install rate-limits handled at bucket edge, not by GitHub's unpredictable anti-abuse behaviour on release download URLs.
- **Can decide public later** — nothing is one-way.

### Cons

- **New infrastructure to maintain.** One more IAM role, one more GH Actions step, one more URL to remember to rotate if we ever want to change buckets.
- **Ongoing (small) cost.** ~$1-3/mo at anticipated scale. Grows with downloads.
- **More moving parts to break on release day.** CI upload can fail independently of `tauri build`.
- **Tauri updater config change.** `tauri.conf.json:27` endpoint becomes the bucket URL; affects every install going forward.

### Effort

- Set up bucket + IAM: 1 hour.
- Wire GH Actions upload step: 1-2 hours.
- Verify round-trip: 1 hour.
- **Ongoing per release:** zero (CI automated).

---

## Recommendation

**Option A**, provided the secret scan comes back clean.

Rationale:
1. The repo's value is the product + IKAROS relationship + signed binary, not the code. Open-sourcing the shell is low-regret.
2. Ongoing cost is zero, matches Tauri's default, no new failure modes introduced.
3. If Moe later decides there's a strategic reason for private (e.g. a proprietary skill or client-specific module), individual modules can be extracted into private sub-repos referenced as git submodules or private npm packages.

Fall back to **Option B (GCS)** if any of the following are true:
- Secret scan finds credentials that can't be cleanly rotated (e.g. a long-lived customer API key embedded historically).
- The commit history contains client-confidential project details that can't be rewritten without breaking history for internal forks.
- Strategic product reasons (e.g. plans to license the source commercially).

---

## Implementation

Once Moe picks:

**If A:**
1. Run `gitleaks` + `trufflehog` end-to-end. Capture output in `.output/2026-04-17-pre-public-secret-scan.md`.
2. Resolve every hit (rotate + commit cleanup, or accept + document why safe).
3. Flip visibility via repo Settings.
4. Confirm `curl -I https://github.com/IKAROSgit/ikrs-workspace/releases/latest/download/latest.json` returns 200 or 404-not-yet-released (either is correct; the point is non-403).
5. No `tauri.conf.json` change required.

**If B:**
1. Create bucket `gs://ikrs-workspace-releases-public/` with anonymous-read, private-write.
2. Add `GCS_WRITE_SA_KEY` to GH Secrets.
3. Add release-upload step to `.github/workflows/ci.yml` after `tauri-action@v0`:
   ```yaml
   - name: Upload to public CDN
     if: github.ref_type == 'tag'
     env:
       GOOGLE_APPLICATION_CREDENTIALS: ${{ secrets.GCS_WRITE_SA_KEY }}
     run: |
       gsutil cp src-tauri/target/release/bundle/dmg/*.dmg gs://ikrs-workspace-releases-public/${{ github.ref_name }}/
       gsutil cp src-tauri/target/release/bundle/macos/latest.json gs://ikrs-workspace-releases-public/latest.json
   ```
4. Update `tauri.conf.json:27` endpoint to `https://storage.googleapis.com/ikrs-workspace-releases-public/latest.json`.
5. Verify round-trip.

---

## What I'm doing in the meantime

No-op. Waiting on Moe's A/B decision before touching `tauri.conf.json:27` again. Until then, the updater points at the GitHub private-repo URL and will 404 on actual releases — acceptable because no release has happened yet.
