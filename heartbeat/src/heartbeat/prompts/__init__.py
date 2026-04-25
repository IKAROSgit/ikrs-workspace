"""Versioned prompt templates.

``tick_prompt_v1.txt`` is the default. Bump to v2 when the schema
changes; keep the old file in place so old telemetry rows can be
retraced to the prompt that produced them.
"""

from __future__ import annotations

from importlib import resources
from typing import TYPE_CHECKING

from heartbeat.signals.base import SignalsBundle

if TYPE_CHECKING:
    pass


CURRENT_PROMPT_VERSION = "tick_prompt.v1"


# Cap on vault changes rendered into the prompt. Mass-change events
# (git checkout, backup restore) can produce 100K+ paths in one tick;
# rendering all of them blows past Gemini's context window AND wastes
# tokens for no benefit (the model's reasoning doesn't scale with N
# files). Per E.4 post-code challenge finding #6.
_MAX_VAULT_CHANGES_IN_PROMPT = 200


def load_prompt_template(version: str = CURRENT_PROMPT_VERSION) -> str:
    """Load the raw prompt template text for the given version.

    ``version`` is the value stored on telemetry's ``promptVersion``
    field. Map ``"tick_prompt.v1"`` → ``tick_prompt_v1.txt`` etc.
    """

    filename_map = {
        "tick_prompt.v1": "tick_prompt_v1.txt",
    }
    filename = filename_map.get(version)
    if filename is None:
        raise ValueError(
            f"unknown prompt version {version!r}. Known: {sorted(filename_map)}"
        )
    return resources.files("heartbeat.prompts").joinpath(filename).read_text(
        encoding="utf-8"
    )


def render_tick_prompt(
    template: str,
    *,
    tick_ts: str,
    tenant_id: str,
    engagement_id: str,
    last_tick_ts: str,
    recent_action_summaries: list[str],
    calendar_lookahead_hours: int,
    gmail_lookback_hours: int,
    bundle: SignalsBundle,
) -> str:
    """Fill the template with this tick's context.

    ``recent_action_summaries`` are natural-language one-liners (per
    ``actions.action_summary_line``) so the LLM has content-level
    context for dedupe — not opaque hex IDs.

    Each block (calendar / gmail / vault / errors) is rendered as a
    bullet list so the model can scan it without having to parse our
    dataclass shapes. When a signal is missing (collector disabled or
    failed), the block reads ``"(not collected this tick)"``.
    """

    return template.format(
        tick_ts=tick_ts,
        tenant_id=tenant_id,
        engagement_id=engagement_id,
        last_tick_ts=last_tick_ts or "(first run)",
        recent_actions_block=_render_recent_actions(recent_action_summaries),
        calendar_lookahead_hours=calendar_lookahead_hours,
        gmail_lookback_hours=gmail_lookback_hours,
        calendar_block=_render_calendar(bundle),
        gmail_block=_render_gmail(bundle),
        vault_block=_render_vault(bundle),
        errors_block=_render_errors(bundle),
    )


def _render_recent_actions(summaries: list[str]) -> str:
    if not summaries:
        return "(none)"
    return "\n".join(f"- {s}" for s in summaries)


def _render_calendar(bundle: SignalsBundle) -> str:
    if bundle.calendar is None:
        return "(not collected this tick)"
    if not bundle.calendar.upcoming_events:
        return "(no upcoming events)"
    lines: list[str] = []
    for event in bundle.calendar.upcoming_events:
        all_day = " [all-day]" if event.is_all_day else ""
        loc = f" @ {event.location}" if event.location else ""
        attendees = (
            f" with {', '.join(event.attendees[:3])}"
            + ("…" if len(event.attendees) > 3 else "")
            if event.attendees
            else ""
        )
        lines.append(
            f"- {event.start} → {event.end}{all_day}: {event.summary}{loc}{attendees}"
        )
    return "\n".join(lines)


def _render_gmail(bundle: SignalsBundle) -> str:
    if bundle.gmail is None:
        return "(not collected this tick)"
    if not bundle.gmail.threads:
        return "(no unread or starred threads)"
    lines: list[str] = []
    for thread in bundle.gmail.threads:
        flags = []
        if thread.is_unread:
            flags.append("unread")
        if thread.is_starred:
            flags.append("starred")
        flag_str = f" [{'/'.join(flags)}]" if flags else ""
        lines.append(
            f"- {thread.received_at} from {thread.sender}{flag_str}: "
            f"{thread.subject} — {thread.snippet[:120]}"
        )
    return "\n".join(lines)


def _render_vault(bundle: SignalsBundle) -> str:
    if bundle.vault is None:
        return "(not collected this tick)"
    if not bundle.vault.changed_files:
        return "(no vault changes)"
    changes = bundle.vault.changed_files
    truncated = len(changes) > _MAX_VAULT_CHANGES_IN_PROMPT
    head = changes[:_MAX_VAULT_CHANGES_IN_PROMPT]
    lines: list[str] = [
        f"- [{c.change_type}] {c.path} ({c.size_bytes} bytes)" for c in head
    ]
    if truncated:
        remaining = len(changes) - _MAX_VAULT_CHANGES_IN_PROMPT
        lines.append(
            f"- (plus {remaining} more files changed — likely a mass-change "
            "event such as a `git checkout` or backup restore; truncated "
            "to keep the prompt within the model's context budget)"
        )
    return "\n".join(lines)


def _render_errors(bundle: SignalsBundle) -> str:
    if not bundle.errors:
        return "(none)"
    lines = [f"- {e.source}: {e.error_code} — {e.message}" for e in bundle.errors]
    return "\n".join(lines)
