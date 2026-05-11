"""OpenRouter adapter — gateway to Kimi K2, DeepSeek, and friends.

Why OpenRouter and not direct Gemini: the rest of the IKAROS VM
stack (Elara, Athena, Helios, etc. under OpenClaw) already runs
through OpenRouter via a single shared `OPENROUTER_API_KEY` with a
spend cap. Routing the heartbeat through the same gateway gives us
one bill, one rotation point, and access to whatever model
performs best per-tick without changing infrastructure.

The Tier II prompt was authored for Gemini's strict
`response_json_schema`, but most OpenRouter upstreams (Moonshot,
DeepSeek) support only the looser `response_format: {type:
"json_object"}`. We use json_object mode and inject the schema
into a synthetic system instruction; the tick already parses +
validates the returned JSON, so this keeps the contract identical
without depending on per-upstream JSON-schema fidelity.

Transport: plain `requests` (already a heartbeat dep for the
Telegram bot). Keeps the adapter dependency-free vs. dragging in
the `openai` SDK just for one HTTP call per tick.
"""

from __future__ import annotations

import json
import logging
from typing import Any

import requests

from heartbeat.config import LlmConfig, env_or
from heartbeat.llm.base import LlmClient, LlmError, LlmRequest, LlmResponse

logger = logging.getLogger("heartbeat.llm.openrouter")

_DEFAULT_BASE_URL = "https://openrouter.ai/api/v1"
_TIMEOUT_SECONDS = 120  # one tick budget; OpenClaw uses similar


class OpenRouterClient(LlmClient):
    """Sync OpenRouter adapter, one instance per heartbeat process."""

    def __init__(
        self,
        config: LlmConfig,
        *,
        _session: requests.Session | None = None,
    ) -> None:
        self._config = config
        self._session = _session or requests.Session()

        api_key = env_or("OPENROUTER_API_KEY")
        if not api_key:
            raise LlmError(
                "OPENROUTER_API_KEY missing from environment. Populate "
                "/etc/ikrs-heartbeat/secrets.env on the VM (mirror the value "
                "from ~/.openclaw/.env.providers), or export it before running "
                "locally.",
                error_code="missing_api_key",
            )
        self._api_key = api_key

        base = env_or("OPENROUTER_BASE_URL") or _DEFAULT_BASE_URL
        self._base_url = base.rstrip("/")

    def generate(self, request: LlmRequest) -> LlmResponse:
        model = _strip_openrouter_prefix(request.model or self._config.model)
        temperature = (
            request.temperature
            if request.temperature is not None
            else self._config.temperature
        )
        max_output_tokens = (
            request.max_output_tokens
            if request.max_output_tokens is not None
            else self._config.max_output_tokens
        )

        messages = _build_messages(request)
        payload: dict[str, Any] = {
            "model": model,
            "messages": messages,
            "temperature": temperature,
            "max_tokens": max_output_tokens,
        }
        if request.response_mime_type == "application/json":
            # Universal JSON mode across OpenRouter upstreams. Strict
            # `json_schema` is upstream-specific (Moonshot supports it,
            # DeepSeek doesn't reliably) — picking `json_object` keeps
            # the adapter model-agnostic. The schema is already in the
            # system prompt (via _build_messages), so the model has
            # enough structure to comply.
            payload["response_format"] = {"type": "json_object"}

        url = f"{self._base_url}/chat/completions"
        headers = {
            "Authorization": f"Bearer {self._api_key}",
            "Content-Type": "application/json",
            # OpenRouter recommends these for attribution + per-app
            # rate-limit accounting. Neither is secret.
            "HTTP-Referer": "https://ikaros.ae",
            "X-Title": "IKAROS Heartbeat",
        }

        try:
            resp = self._session.post(
                url, headers=headers, json=payload, timeout=_TIMEOUT_SECONDS
            )
        except requests.RequestException as exc:
            raise LlmError(
                f"openrouter request failed: {type(exc).__name__}: {exc}",
                error_code="network_error",
            ) from exc

        if resp.status_code == 401:
            raise LlmError(
                "openrouter rejected the API key (HTTP 401). Rotate "
                "OPENROUTER_API_KEY or check the spend cap on the dashboard.",
                error_code="invalid_api_key",
            )
        if resp.status_code == 429:
            raise LlmError(
                "openrouter rate-limited the heartbeat (HTTP 429). Tier II "
                "ticks are hourly so this usually means another process on "
                "the same key blew the budget.",
                error_code="rate_limited",
            )
        if resp.status_code >= 400:
            raise LlmError(
                f"openrouter returned HTTP {resp.status_code}: "
                f"{_safe_body_preview(resp)}",
                error_code="llm_call_failed",
            )

        try:
            data = resp.json()
        except ValueError as exc:
            raise LlmError(
                f"openrouter response was not JSON: {_safe_body_preview(resp)}",
                error_code="empty_response",
            ) from exc

        text = _extract_text(data)
        prompt_tokens, output_tokens, total_tokens = _extract_usage(
            data.get("usage"), request.prompt, text
        )
        # OpenRouter sometimes echoes `id` for the chosen upstream
        # model; prefer it for telemetry, fall back to the requested one.
        actual_model = str(data.get("model") or model)

        return LlmResponse(
            text=text,
            tokens_used=total_tokens,
            prompt_tokens=prompt_tokens,
            output_tokens=output_tokens,
            model=actual_model,
        )


def _strip_openrouter_prefix(model: str) -> str:
    """OpenClaw uses ``openrouter/<provider>/<model>`` everywhere
    (e.g. ``openrouter/moonshotai/kimi-k2-0905``). The OpenRouter HTTP
    API itself expects only the ``<provider>/<model>`` suffix. Accept
    both forms so operators can copy/paste from openclaw.json without
    re-formatting."""

    if model.startswith("openrouter/"):
        return model[len("openrouter/") :]
    return model


def _build_messages(request: LlmRequest) -> list[dict[str, str]]:
    """Build the OpenAI-style ``messages`` list.

    When the caller asks for JSON output, prepend a system message
    that pins the response to JSON and embeds the schema so the
    model has something to comply with. Without it, json_object mode
    will produce valid JSON but with arbitrary keys."""

    messages: list[dict[str, str]] = []
    system_chunks: list[str] = []
    if request.system_instruction:
        system_chunks.append(request.system_instruction)
    if (
        request.response_mime_type == "application/json"
        and request.response_json_schema is not None
    ):
        schema_text = json.dumps(request.response_json_schema, ensure_ascii=False)
        system_chunks.append(
            "You MUST respond with a single JSON object that conforms to "
            "this JSON Schema. No prose, no markdown fences, no commentary "
            "outside the JSON.\n\nSchema:\n" + schema_text
        )
    if system_chunks:
        messages.append({"role": "system", "content": "\n\n".join(system_chunks)})
    messages.append({"role": "user", "content": request.prompt})
    return messages


def _extract_text(data: dict[str, Any]) -> str:
    """Pull the assistant text out of the OpenAI-shaped response."""

    choices = data.get("choices")
    if not isinstance(choices, list) or not choices:
        raise LlmError(
            "openrouter response had no choices array",
            error_code="empty_response",
        )
    first = choices[0]
    if not isinstance(first, dict):
        raise LlmError(
            "openrouter choices[0] was not a dict",
            error_code="empty_response",
        )
    msg = first.get("message")
    if isinstance(msg, dict):
        content = msg.get("content")
        if isinstance(content, str) and content:
            return content
        # Some OpenRouter providers stream `content` as a list of
        # content-blocks (mirroring Anthropic's shape). Concatenate
        # the text parts in order.
        if isinstance(content, list):
            parts: list[str] = []
            for block in content:
                if isinstance(block, dict):
                    if block.get("type") == "text" and isinstance(
                        block.get("text"), str
                    ):
                        parts.append(block["text"])
                    elif isinstance(block.get("content"), str):
                        parts.append(block["content"])
            joined = "".join(parts).strip()
            if joined:
                return joined
    # Fall back to legacy `text` (older completions shape) if present.
    if isinstance(first.get("text"), str) and first["text"]:
        return first["text"]
    raise LlmError(
        "openrouter response had empty content",
        error_code="empty_response",
    )


def _extract_usage(
    usage: Any, prompt: str, text: str
) -> tuple[int, int, int]:
    """Best-effort token-count extraction.

    OpenAI-compat APIs use ``prompt_tokens`` / ``completion_tokens`` /
    ``total_tokens``. OpenRouter mirrors that for most upstreams; if a
    proxy strips it we fall back to a rough char/4 estimate so
    telemetry still shows a non-zero number."""

    if not isinstance(usage, dict):
        return _estimate_tokens(prompt, text)
    prompt_tokens = _coerce_int(usage.get("prompt_tokens"))
    output_tokens = _coerce_int(usage.get("completion_tokens"))
    total_tokens = _coerce_int(usage.get("total_tokens"))
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
    prompt_est = max(1, len(prompt) // 4)
    output_est = max(1, len(text) // 4)
    logger.warning(
        "openrouter response missing usage block; estimating tokens "
        "(prompt~%d, output~%d). Telemetry tokensUsed will be approximate.",
        prompt_est,
        output_est,
    )
    return prompt_est, output_est, prompt_est + output_est


def _safe_body_preview(resp: requests.Response) -> str:
    """Return at most 400 chars of the response body, guaranteed to
    not contain the API key (which we never echo back into errors)."""

    try:
        body = resp.text or ""
    except Exception:  # noqa: BLE001 — defensive: resp.text rarely raises
        return f"<unreadable body, status={resp.status_code}>"
    body = body.replace("\n", " ").strip()
    if len(body) > 400:
        body = body[:397] + "..."
    return body
