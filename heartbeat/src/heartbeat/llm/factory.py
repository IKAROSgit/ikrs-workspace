"""LLM adapter factory. Dispatches on ``LlmConfig.provider``.

Centralised here (rather than in each adapter) so the tick orchestrator
imports a single function and never knows what provider is configured —
which keeps E.4 stable across future provider additions.
"""

from __future__ import annotations

from heartbeat.config import LlmConfig
from heartbeat.llm.base import LlmClient


def make_llm_client(config: LlmConfig) -> LlmClient:
    """Return a configured adapter.

    Raises ``ValueError`` for unknown providers and ``NotImplementedError``
    for providers we recognise but haven't implemented yet (currently:
    "claude").
    """

    provider = config.provider
    if provider == "gemini":
        # Local import so test suites that mock out gemini never need to
        # import the real google-genai package.
        from heartbeat.llm.gemini import GeminiClient

        return GeminiClient(config)
    if provider == "claude":
        raise NotImplementedError(
            "claude adapter is deferred until first commercial tenant "
            "brings their own ANTHROPIC_API_KEY (spec §Out of scope)."
        )
    raise ValueError(f"unknown llm.provider: {provider!r}")
