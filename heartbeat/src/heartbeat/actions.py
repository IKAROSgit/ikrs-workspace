"""Typed actions emitted by the tick orchestrator.

The LLM returns a JSON document conforming to ``TICK_RESPONSE_SCHEMA``;
the orchestrator parses it into a list of ``Action`` objects. E.5
(Firestore + Telegram outputs) consumes these and dispatches each to the
appropriate sink.

Why a discriminated union over a single shape: each action has a
different downstream fate (Firestore Kanban write vs. memory append vs.
Telegram push). Keeping them typed lets E.5 dispatch with ``isinstance``
or ``match`` instead of stringly-typed branching, and lets mypy catch
shape drift when the schema changes.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Literal

# JSON-schema literal types
ActionType = Literal["kanban_task", "memory_update", "telegram_push"]
Priority = Literal["low", "medium", "high", "urgent"]
Urgency = Literal["info", "warning", "urgent"]


@dataclass(frozen=True)
class KanbanTaskAction:
    """Add a task to the engagement's Kanban board (Firestore tasks/*)."""

    type: Literal["kanban_task"]
    id: str  # opaque UUID — orchestrator generates a fresh hex per emission
    title: str
    description: str
    priority: Priority
    rationale: str  # one-liner explaining why this matters now
    # ISO-8601 timestamp set at re-key time. E.5's Firestore write uses
    # this as the document's createdAt — independent of Firestore's own
    # serverTimestamp so the order is stable even if Firestore is
    # degraded.
    emitted_at: str = ""


@dataclass(frozen=True)
class MemoryUpdateAction:
    """Append a note to the engagement's _memory/heartbeat-log.jsonl."""

    type: Literal["memory_update"]
    id: str
    note: str
    tags: list[str] = field(default_factory=list)
    emitted_at: str = ""


@dataclass(frozen=True)
class TelegramPushAction:
    """Send a one-shot Telegram message to the operator's bot/chat."""

    type: Literal["telegram_push"]
    id: str
    message: str
    urgency: Urgency
    emitted_at: str = ""


Action = KanbanTaskAction | MemoryUpdateAction | TelegramPushAction


def action_summary_line(action: Action) -> str:
    """One-line natural-language summary for prompt-context dedupe.

    Fed back to the LLM next tick (instead of opaque UUID hex) so the
    model has natural-language context about what it emitted last
    hour and can avoid repeating itself. Per E.4 post-code challenge
    finding #1.
    """

    if isinstance(action, KanbanTaskAction):
        return f"kanban_task[{action.priority}]: {action.title}"
    if isinstance(action, MemoryUpdateAction):
        snippet = action.note[:80]
        return f"memory_update: {snippet}"
    if isinstance(action, TelegramPushAction):
        snippet = action.message[:80]
        return f"telegram_push[{action.urgency}]: {snippet}"
    # Defensive — unreachable while Action is the union above.
    return f"unknown_action: {action!r}"


# JSON schema the LLM must conform to. Used as
# ``GenerateContentConfig.response_json_schema`` so Gemini enforces it
# server-side. Fields kept minimal — every required field MUST always be
# easy for the model to fill in (no edge-case-required strings that lead
# to refusals or empty responses).
TICK_RESPONSE_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {
        "summary": {
            "type": "string",
            "description": (
                "1-2 sentence summary of what changed since the last "
                "tick and the most important thing the operator should "
                "know right now."
            ),
        },
        "actions": {
            "type": "array",
            "description": (
                "Zero or more typed actions. Empty array is the right "
                "answer when nothing actionable has changed."
            ),
            "items": {
                "type": "object",
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["kanban_task", "memory_update", "telegram_push"],
                    },
                    "id": {
                        "type": "string",
                        "description": (
                            "UUID-like opaque identifier the heartbeat "
                            "uses for dedupe. Caller can ignore — the "
                            "orchestrator regenerates IDs server-side."
                        ),
                    },
                    "title": {"type": "string"},
                    "description": {"type": "string"},
                    "priority": {
                        "type": "string",
                        "enum": ["low", "medium", "high", "urgent"],
                    },
                    "rationale": {"type": "string"},
                    "note": {"type": "string"},
                    "tags": {"type": "array", "items": {"type": "string"}},
                    "message": {"type": "string"},
                    "urgency": {
                        "type": "string",
                        "enum": ["info", "warning", "urgent"],
                    },
                },
                "required": ["type", "id"],
            },
        },
    },
    "required": ["summary", "actions"],
}


class ActionParseError(ValueError):
    """Raised when the LLM returns JSON we cannot map to typed actions.

    Caught by the tick orchestrator and recorded as ``error_code =
    "action_parse_failed"`` in telemetry. Tick still completes with
    ``actionsEmitted = 0``.
    """


def parse_actions(raw: dict[str, Any]) -> tuple[str, list[Action]]:
    """Parse a Gemini-returned dict into ``(summary, [Action, ...])``.

    Defensive: tolerates extra fields, missing optional fields, and
    enum drift (an unknown ``type`` is dropped with a logged warning,
    not raised — partial action lists are better than a failed tick).
    """

    if not isinstance(raw, dict):
        raise ActionParseError(f"top-level response is not an object: {type(raw)}")
    if "summary" not in raw:
        raise ActionParseError("response missing required key 'summary'")
    if "actions" not in raw:
        raise ActionParseError("response missing required key 'actions'")

    summary = str(raw["summary"])
    actions_raw = raw["actions"]
    if not isinstance(actions_raw, list):
        raise ActionParseError(
            f"'actions' must be a list, got {type(actions_raw).__name__}"
        )

    actions: list[Action] = []
    for i, item in enumerate(actions_raw):
        action = _parse_one(item, i)
        if action is not None:
            actions.append(action)
    return summary, actions


def _parse_one(item: Any, index: int) -> Action | None:
    """Parse one action dict. Returns None on a recognisable-but-unsupported
    type (e.g. ``"type": "send_email"`` from a future schema bump) so the
    rest of the list still flows through."""

    if not isinstance(item, dict):
        return None
    type_str = str(item.get("type", ""))
    action_id = str(item.get("id", ""))
    if not action_id:
        return None

    if type_str == "kanban_task":
        return KanbanTaskAction(
            type="kanban_task",
            id=action_id,
            title=str(item.get("title", "")),
            description=str(item.get("description", "")),
            priority=_coerce_priority(item.get("priority")),
            rationale=str(item.get("rationale", "")),
        )
    if type_str == "memory_update":
        tags_raw = item.get("tags") or []
        tags = [str(t) for t in tags_raw if isinstance(t, (str, int, float))]
        return MemoryUpdateAction(
            type="memory_update",
            id=action_id,
            note=str(item.get("note", "")),
            tags=tags,
        )
    if type_str == "telegram_push":
        return TelegramPushAction(
            type="telegram_push",
            id=action_id,
            message=str(item.get("message", "")),
            urgency=_coerce_urgency(item.get("urgency")),
        )
    # Unknown action type — drop silently. Logged at orchestrator level.
    return None


def _coerce_priority(value: Any) -> Priority:
    if value == "low":
        return "low"
    if value == "high":
        return "high"
    if value == "urgent":
        return "urgent"
    # "medium" is the default fallback for unknown / missing.
    return "medium"


def _coerce_urgency(value: Any) -> Urgency:
    if value == "warning":
        return "warning"
    if value == "urgent":
        return "urgent"
    return "info"
