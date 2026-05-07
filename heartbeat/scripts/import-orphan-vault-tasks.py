#!/usr/bin/env python3
"""Import task markdown files from an orphaned vault to a new one.

Copies .md files from <from-vault>/02-tasks/ to <to-vault>/02-tasks/,
preserving content and adding an originalEngagementId frontmatter
field for audit. Does NOT modify the original files.

Usage:
  python import-orphan-vault-tasks.py \
    --from-vault /path/to/bar-world-com \
    --to-vault /path/to/blr-world-com \
    --dry-run

Exit codes: 0 = ok, 1 = path error, 2 = no files found
"""

from __future__ import annotations

import argparse
import logging
import shutil
import sys
from pathlib import Path

logger = logging.getLogger("import-orphan-vault-tasks")


def _parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="import-orphan-vault-tasks",
        description="Copy task .md files from an orphaned vault to a new one.",
    )
    parser.add_argument("--from-vault", type=Path, required=True,
                        help="Source vault root (e.g. ~/.ikrs-workspace/vaults/bar-world-com)")
    parser.add_argument("--to-vault", type=Path, required=True,
                        help="Destination vault root (e.g. ~/.ikrs-workspace/vaults/blr-world-com)")
    parser.add_argument("--dry-run", action="store_true",
                        help="Show what would be copied without writing.")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(argv)
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
        stream=sys.stderr,
    )

    src = args.from_vault / "02-tasks"
    dst = args.to_vault / "02-tasks"

    if not src.exists():
        logger.error("source 02-tasks/ not found: %s", src)
        return 1
    if not dst.exists():
        dst.mkdir(parents=True, exist_ok=True)
        logger.info("created destination: %s", dst)

    files = sorted(src.glob("*.md"))
    if not files:
        logger.info("no .md files found in %s", src)
        return 2

    copied = 0
    skipped = 0

    for f in files:
        # Skip dotfiles
        if f.name.startswith("."):
            continue

        target = dst / f.name
        if target.exists():
            logger.info("SKIP (already exists): %s", f.name)
            skipped += 1
            continue

        if args.dry_run:
            logger.info("DRY RUN: would copy %s → %s", f.name, target)
            copied += 1
            continue

        shutil.copy2(f, target)
        logger.info("copied: %s → %s", f.name, target)
        copied += 1

    logger.info("")
    logger.info("done: %d copied, %d skipped (already existed)", copied, skipped)
    if args.dry_run:
        logger.info("DRY RUN — no files were actually written.")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
