"""LLM adapters.

Public surface (E.2):
- ``LlmRequest`` / ``LlmResponse`` — provider-agnostic request/response shape.
- ``LlmClient`` — abstract protocol every adapter implements.
- ``LlmError`` — single exception family the tick orchestrator catches.
- ``make_llm_client(config)`` — factory. Selects an adapter from
  ``config.llm.provider``.

Adapters live in submodules:
- ``heartbeat.llm.gemini`` — Tier II default (E.2).
- ``heartbeat.llm.claude`` — deferred until first commercial tenant brings
  their own ``ANTHROPIC_API_KEY``.
"""

from heartbeat.llm.base import LlmClient, LlmError, LlmRequest, LlmResponse
from heartbeat.llm.factory import make_llm_client

__all__ = [
    "LlmClient",
    "LlmError",
    "LlmRequest",
    "LlmResponse",
    "make_llm_client",
]
