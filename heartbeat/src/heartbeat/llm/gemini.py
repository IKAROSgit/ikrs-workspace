"""Gemini adapter (Tier II default).

Uses the ``google-genai`` SDK (the unified Gen AI client; the legacy
``google-generativeai`` was archived 2025-12-16). One client per process,
re-used across ticks. API key auto-loaded from ``GEMINI_API_KEY``.

Future-proofing notes:
- For Vertex AI, set ``GOOGLE_GENAI_USE_VERTEXAI=True`` + project/location
  env vars; the call site below stays identical.
- Async path available via ``client.aio.models.generate_content`` if a
  future tick needs to fan out — we don't today (single LLM call per
  hourly tick).
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

from heartbeat.config import LlmConfig, env_or
from heartbeat.llm.base import LlmClient, LlmError, LlmRequest, LlmResponse

if TYPE_CHECKING:  # pragma: no cover
    from google.genai import Client as GenaiClient  # noqa: F401

logger = logging.getLogger("heartbeat.llm.gemini")


class GeminiClient(LlmClient):
    """Sync Gemini adapter. One instance per heartbeat process."""

    def __init__(self, config: LlmConfig, *, _client: Any | None = None) -> None:
        """Build the adapter.

        ``_client`` is a test seam: pass a fake to bypass the real
        ``google.genai.Client`` instantiation. Production callers always
        pass ``config`` only.
        """

        self._config = config
        if _client is not None:
            self._client = _client
            return

        api_key = env_or("GEMINI_API_KEY")
        if not api_key:
            raise LlmError(
                "GEMINI_API_KEY missing from environment. Populate "
                "/etc/ikrs-heartbeat/secrets.env on the VM, or export it "
                "before running locally.",
                error_code="missing_api_key",
            )

        # Local import keeps pytest fast and lets factory-level mocks bypass
        # the SDK entirely.
        from google import genai

        self._client = genai.Client(api_key=api_key)

    def generate(self, request: LlmRequest) -> LlmResponse:
        """One synchronous Gemini call. Raises ``LlmError`` on any failure."""

        # Local import — same rationale as in __init__.
        try:
            from google.genai import types
        except ImportError as exc:  # pragma: no cover - pre-install sanity
            raise LlmError(
                f"google-genai not importable: {exc}",
                error_code="sdk_import_failed",
            ) from exc

        gen_config = types.GenerateContentConfig(
            system_instruction=request.system_instruction,
            temperature=request.temperature,
            max_output_tokens=request.max_output_tokens,
        )

        try:
            resp = self._client.models.generate_content(
                model=request.model,
                contents=request.prompt,
                config=gen_config,
            )
        except Exception as exc:  # noqa: BLE001 — narrow types vary across SDK versions
            # Surface the underlying error message without leaking the API
            # key (the SDK does not include it in exception strings, but
            # we belt-and-brace by stringifying via the exception type).
            raise LlmError(
                f"gemini generate_content failed: {type(exc).__name__}: {exc}",
                error_code="llm_call_failed",
            ) from exc

        text = _extract_text(resp)
        usage = getattr(resp, "usage_metadata", None)
        prompt_tokens, output_tokens, total_tokens = _extract_usage(usage, request.prompt, text)

        return LlmResponse(
            text=text,
            tokens_used=total_tokens,
            prompt_tokens=prompt_tokens,
            output_tokens=output_tokens,
            model=request.model,
        )


def _extract_text(resp: Any) -> str:
    """Pull text out of a Gemini response.

    The SDK's ``.text`` accessor concatenates all parts. If the response
    was blocked or empty, ``.text`` may raise; surface that as an
    ``LlmError`` so the tick orchestrator records ``error_code``.
    """

    try:
        text = resp.text
    except Exception as exc:  # noqa: BLE001 — SDK raises various types here
        raise LlmError(
            f"gemini response had no text: {type(exc).__name__}: {exc}",
            error_code="empty_response",
        ) from exc
    if text is None:
        raise LlmError("gemini returned None text", error_code="empty_response")
    return str(text)


def _extract_usage(usage: Any, prompt: str, text: str) -> tuple[int, int, int]:
    """Best-effort extraction of token counts.

    Gemini exposes ``prompt_token_count`` / ``candidates_token_count`` /
    ``total_token_count`` on every successful response. If a fixture or
    a future SDK release changes the shape, fall back to a rough
    char/4 estimate so telemetry still records a non-zero number.
    """

    if usage is None:
        return _estimate_tokens(prompt, text)

    prompt_tokens = _coerce_int(getattr(usage, "prompt_token_count", None))
    output_tokens = _coerce_int(getattr(usage, "candidates_token_count", None))
    total_tokens = _coerce_int(getattr(usage, "total_token_count", None))
    if total_tokens == 0 and (prompt_tokens or output_tokens):
        total_tokens = prompt_tokens + output_tokens
    if total_tokens == 0:
        return _estimate_tokens(prompt, text)
    return prompt_tokens, output_tokens, total_tokens


def _coerce_int(value: Any) -> int:
    if value is None:
        return 0
    try:
        return int(value)
    except (TypeError, ValueError):
        return 0


def _estimate_tokens(prompt: str, text: str) -> tuple[int, int, int]:
    """~4 chars per token rule of thumb. Logged so we know the SDK didn't
    give us real counts on this tick."""

    prompt_est = max(1, len(prompt) // 4)
    output_est = max(1, len(text) // 4)
    logger.warning(
        "gemini response missing usage_metadata; estimating tokens "
        "(prompt~%d, output~%d). Telemetry tokensUsed will be approximate.",
        prompt_est,
        output_est,
    )
    return prompt_est, output_est, prompt_est + output_est
