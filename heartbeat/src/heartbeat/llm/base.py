"""Provider-agnostic LLM interface.

Why a Protocol instead of an ABC: keeps adapters cheap to add in tests
(any object with a ``.generate(req)`` method satisfies it) and means the
tick orchestrator never has to import a concrete class.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Protocol


@dataclass(frozen=True)
class LlmRequest:
    """One synchronous request to an LLM.

    Kept tiny on purpose. The tick orchestrator builds these from the
    rendered prompt template (E.4); adapters translate to provider-specific
    schemas internally.
    """

    prompt: str
    system_instruction: str | None = None
    # Generation knobs. Defaults match LlmConfig defaults so a caller can
    # always pass an LlmRequest(prompt=...) and get sane behaviour.
    model: str = "gemini-2.5-pro"
    temperature: float = 0.2
    max_output_tokens: int = 4096


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
