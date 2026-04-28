"""Entry point for the IKAROS heartbeat tick.

Invoked by systemd timer (``ikrs-heartbeat.timer``) every hour, or manually
for dry-run / smoke tests.
"""

from __future__ import annotations

import argparse
import logging
import sys
from dataclasses import replace
from pathlib import Path

from heartbeat.config import EngagementConfig, HeartbeatConfig, load_config
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


def _config_for_engagement(
    base: HeartbeatConfig, eng: EngagementConfig
) -> HeartbeatConfig:
    """Derive a per-engagement config from the base config.

    Swaps engagement_id, vault_root, and audit_log_path to point at
    this engagement's vault. All other settings (LLM, signals, outputs)
    are shared across engagements.
    """
    audit_log_path = eng.vault_root / "_memory" / "heartbeat-log.jsonl"
    return replace(
        base,
        engagement_id=eng.id,
        vault_root=eng.vault_root,
        audit_log_path=audit_log_path,
    )


def _print_dry_run_plan(config: HeartbeatConfig) -> None:
    """Print what a real tick *would* do, without doing it."""

    logger.info("DRY RUN — no network calls, no writes.")
    logger.info("tenant_id=%s", config.tenant_id)
    logger.info("engagements (%d):", len(config.engagements))
    for eng in config.engagements:
        logger.info("  - id=%s vault_root=%s", eng.id, eng.vault_root)
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

    # Phase F: iterate engagements. Each engagement gets its own tick +
    # dispatch cycle with error isolation — one broken engagement must
    # not block others.
    secrets = OutputSecrets.from_env()
    any_error = False

    for eng in config.engagements:
        eng_config = _config_for_engagement(config, eng)
        logger.info("--- engagement %s (vault: %s) ---", eng.id, eng.vault_root)

        try:
            result = run_tick(eng_config, token_path=args.token_path)
            _log_tick_summary(result)

            dispatch = dispatch_outputs(eng_config, secrets, result, tier="II")
            _log_dispatch_summary(dispatch)

            if result.status not in {"ok", "no-op"}:
                any_error = True
        except Exception:
            logger.exception("Unhandled error for engagement %s", eng.id)
            any_error = True

    return 1 if any_error else 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
