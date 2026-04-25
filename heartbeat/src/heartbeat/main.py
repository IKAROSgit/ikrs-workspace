"""Entry point for the IKAROS heartbeat tick.

Invoked by systemd timer (``ikrs-heartbeat.timer``) every hour, or manually
for dry-run / smoke tests.
"""

from __future__ import annotations

import argparse
import logging
import sys
from pathlib import Path

from heartbeat.config import HeartbeatConfig, load_config
from heartbeat.outputs import DispatchResult, OutputSecrets, dispatch_outputs
from heartbeat.tick import TickResult, run_tick

logger = logging.getLogger("heartbeat.main")


# Default location of the Mac-side OAuth token, scp'd to the VM by
# install.sh. Spec §Tier II §Auth on VM. E.6's installer writes here.
_DEFAULT_TOKEN_PATH = Path("/etc/ikrs-heartbeat/google-token.json")


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
        "--token-path",
        type=Path,
        default=_DEFAULT_TOKEN_PATH,
        help=(
            "Path to the Google OAuth token.json (scp'd from operator's "
            f"Mac during install). Default: {_DEFAULT_TOKEN_PATH}."
        ),
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


def _log_dispatch_summary(dispatch: DispatchResult) -> None:
    """Log E.5 dispatch outcome at INFO."""

    logger.info(
        "dispatch: dispatched=%d skipped_dedupe=%d failed=%d "
        "telemetry_written=%s audit_lines=%d",
        dispatch.actions_dispatched,
        dispatch.actions_skipped_dedupe,
        dispatch.actions_failed,
        dispatch.telemetry_written,
        dispatch.audit_lines_written,
    )


def _log_tick_summary(result: TickResult) -> None:
    """Log one line per tick at INFO so journalctl is human-scannable."""

    logger.info(
        "tick: status=%s actions=%d duration_ms=%d tokens=%d model=%s error=%s",
        result.status,
        result.actions_emitted,
        result.duration_ms,
        result.tokens_used,
        result.model_used or "(none)",
        result.error_code or "-",
    )
    if result.summary:
        logger.info("tick summary: %s", result.summary)
    for err in result.collector_errors:
        logger.info("collector error: %s/%s — %s", err.source, err.error_code, err.message)


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

    # E.4: produce the tick. E.5: dispatch its outputs.
    result = run_tick(config, token_path=args.token_path)
    _log_tick_summary(result)

    secrets = OutputSecrets.from_env()
    dispatch = dispatch_outputs(config, secrets, result, tier="II")
    _log_dispatch_summary(dispatch)

    # Tick is "successful" if the LLM call landed; dispatch errors are
    # logged separately but don't push exit-code to non-zero (the audit
    # log captures everything for ops). Only a hard tick error returns 1.
    return 0 if result.status in {"ok", "no-op"} else 1


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
