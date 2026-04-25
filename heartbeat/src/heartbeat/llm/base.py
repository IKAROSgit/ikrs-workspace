"""Provider-agnostic LLM interface.

Why a Protocol instead of an ABC: keeps adapters cheap to add in tests
(any object with a ``.generate(req)`` method satisfies it) and means the
tick orchestrator never has to import a concrete class.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Protocol


@dataclass(frozen=True)
class LlmRequest:
    """One synchronous request to an LLM.

    Kept tiny on purpose. The tick orchestrator builds these from the
    rendered prompt template; adapters translate to provider-specific
    schemas internally.

    Generation knobs (model, temperature, max_output_tokens) are optional
    per-request **overrides**. When ``None`` (the default), the adapter
    uses the values bound to its ``LlmConfig`` at construction.

    Structured-output knobs (``response_mime_type`` and
    ``response_json_schema``, both added in E.4) tell the model to return
    strict JSON matching a schema. When provided, ``google-genai`` enforces
    server-side and the response will be a valid JSON document — no need
    for fence-stripping or markdown parsing in the orchestrator. The tick
    uses this so action emission is reliable across runs.
    """

    prompt: str
    system_instruction: str | None = None
    # All three default to None → adapter falls back to bound LlmConfig.
    model: str | None = None
    temperature: float | None = None
    max_output_tokens: int | None = None
    # Structured output (E.4). Both must be set for JSON mode; either
    # being None means free-form text. ``response_json_schema`` is a raw
    # JSON Schema dict (not a pydantic class) so it stays portable across
    # provider adapters.
    response_mime_type: str | None = None
    response_json_schema: dict[str, Any] | None = None


@dataclass(frozen=True)
class LlmResponse:
    """One response back from an LLM.

    ``tokens_used`` is recorded in the ``heartbeat_health`` Firestore doc
    (spec §Telemetry), so it must always be populated — even on the rare
    provider that doesn't expose token counts (in which case the adapter
    should estimate from len(prompt+response)//4 and document the
    assumption).
    """

    text: str
    tokens_used: int
    prompt_tokens: int
    output_tokens: int
    model: str  # echoed back so we can confirm the deployed model in telemetry


class LlmClient(Protocol):
    """Every adapter implements this. One method, sync, in-process."""

    def generate(self, request: LlmRequest) -> LlmResponse:  # pragma: no cover - protocol
        ...


class LlmError(RuntimeError):
    """Raised by adapters when generation fails for any provider-side reason.

    The tick orchestrator catches this and records ``error_code`` on the
    telemetry doc — never crashes the whole service. Subclasses can carry
    structured detail, but stringifying ``LlmError`` should always yield a
    one-line operator-readable message.
    """

    def __init__(self, message: str, *, error_code: str = "llm_error") -> None:
        super().__init__(message)
        self.error_code = error_code
