"""Entry point for the IKAROS heartbeat tick.

Invoked by systemd timer (``ikrs-heartbeat.timer``) every hour, or manually
for dry-run / smoke tests.

Sub-phases:
- E.1 (this commit): wire up ``--dry-run`` so it parses config and prints a
  plan without making any network call. The actual signal collection and LLM
  call land in E.3 / E.4.
"""

from __future__ import annotations

import argparse
import logging
import sys
from pathlib import Path

from heartbeat.config import HeartbeatConfig, load_config

logger = logging.getLogger("heartbeat.main")


def _parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="ikrs-heartbeat",
        description=(
            "IKAROS Tier II heartbeat — one tick of "
            "Gmail/Calendar/vault → Gemini → Firestore."
        ),
    )
    parser.add_argument(
        "--config",
        type=Path,
        default=Path("/etc/ikrs-heartbeat/heartbeat.toml"),
        help="Path to heartbeat.toml (default: /etc/ikrs-heartbeat/heartbeat.toml).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Parse config and print plan; make no network calls and no writes.",
    )
    parser.add_argument(
        "--once",
        action="store_true",
        help="Run a single tick and exit (default behaviour; reserved for future loop mode).",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Verbose logging (DEBUG level).",
    )
    return parser.parse_args(argv)


def _configure_logging(verbose: bool) -> None:
    logging.basicConfig(
        level=logging.DEBUG if verbose else logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
        stream=sys.stderr,
    )


def _print_dry_run_plan(config: HeartbeatConfig) -> None:
    """Print what a real tick *would* do, without doing it."""

    logger.info("DRY RUN — no network calls, no writes.")
    logger.info("tenant_id=%s engagement_id=%s", config.tenant_id, config.engagement_id)
    logger.info("vault_root=%s", config.vault_root)
    logger.info("llm.provider=%s llm.model=%s", config.llm.provider, config.llm.model)
    logger.info("prompt_version=%s", config.prompt_version)
    logger.info(
        "outputs: firestore=%s telegram=%s audit=%s",
        config.outputs.firestore_enabled,
        config.outputs.telegram_enabled,
        config.outputs.audit_enabled,
    )
    logger.info("Would: collect signals → prompt LLM → emit actions → write telemetry.")


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(argv)
    _configure_logging(args.verbose)

    try:
        config = load_config(args.config)
    except FileNotFoundError as exc:
        logger.error("config not found: %s", exc)
        return 2
    except ValueError as exc:
        logger.error("invalid config: %s", exc)
        return 2

    if args.dry_run:
        _print_dry_run_plan(config)
        return 0

    # E.1 stub: real tick lands in E.4 once orchestrator + prompt + outputs
    # are wired up. We refuse to run a non-dry tick today so a misconfigured
    # systemd timer cannot silently no-op against production Firestore.
    logger.error(
        "Real tick not implemented yet (E.4). Re-run with --dry-run, "
        "or wait for sub-phase E.4 to land."
    )
    return 64  # EX_USAGE


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
