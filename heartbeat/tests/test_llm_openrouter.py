"""Tests for the OpenRouter adapter.

Strategy: mock the ``requests.Session.post`` call so the suite never
hits openrouter.ai. The adapter's job is to translate
``LlmRequest``/``LlmResponse`` to/from the OpenAI-compat HTTP payload;
that translation is what we verify here.
"""

from __future__ import annotations

import json
from typing import Any
from unittest.mock import MagicMock

import pytest
import requests

from heartbeat.config import LlmConfig
from heartbeat.llm import LlmError, LlmRequest, LlmResponse, make_llm_client
from heartbeat.llm.openrouter import (
    OpenRouterClient,
    _build_messages,
    _extract_text,
    _extract_usage,
    _strip_openrouter_prefix,
)


def _ok_response(
    *,
    content: Any = "reply!",
    usage: dict[str, int] | None = None,
    model: str = "moonshotai/kimi-k2-0905",
) -> MagicMock:
    """Fake requests.Response: 200 + minimal OpenAI-shape JSON body."""

    body: dict[str, Any] = {
        "id": "gen-test",
        "model": model,
        "choices": [
            {
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop",
            }
        ],
        "usage": usage if usage is not None else {
            "prompt_tokens": 120,
            "completion_tokens": 30,
            "total_tokens": 150,
        },
    }
    resp = MagicMock(spec=requests.Response)
    resp.status_code = 200
    resp.json.return_value = body
    resp.text = json.dumps(body)
    return resp


def _err_response(status: int, body: str = "boom") -> MagicMock:
    resp = MagicMock(spec=requests.Response)
    resp.status_code = status
    resp.text = body
    resp.json.side_effect = ValueError("not json")
    return resp


def _cfg(model: str = "moonshotai/kimi-k2-0905") -> LlmConfig:
    return LlmConfig(
        provider="openrouter",
        model=model,
        temperature=0.2,
        max_output_tokens=4096,
    )


def _client(
    monkeypatch: pytest.MonkeyPatch, session: MagicMock, model: str | None = None
) -> OpenRouterClient:
    monkeypatch.setenv("OPENROUTER_API_KEY", "sk-or-v1-test")
    cfg = _cfg(model) if model else _cfg()
    return OpenRouterClient(cfg, _session=session)


# -------- happy path --------


def test_generate_returns_text_and_tokens(monkeypatch: pytest.MonkeyPatch) -> None:
    session = MagicMock()
    session.post.return_value = _ok_response(content="reply!")
    client = _client(monkeypatch, session)

    resp = client.generate(LlmRequest(prompt="hi"))

    assert isinstance(resp, LlmResponse)
    assert resp.text == "reply!"
    assert resp.tokens_used == 150
    assert resp.prompt_tokens == 120
    assert resp.output_tokens == 30
    assert resp.model == "moonshotai/kimi-k2-0905"


def test_generate_posts_to_chat_completions(monkeypatch: pytest.MonkeyPatch) -> None:
    session = MagicMock()
    session.post.return_value = _ok_response()
    client = _client(monkeypatch, session)

    client.generate(LlmRequest(prompt="hi"))

    session.post.assert_called_once()
    url = session.post.call_args.args[0]
    assert url.endswith("/chat/completions")
    headers = session.post.call_args.kwargs["headers"]
    assert headers["Authorization"] == "Bearer sk-or-v1-test"
    assert headers["Content-Type"] == "application/json"


def test_generate_payload_carries_config_knobs(monkeypatch: pytest.MonkeyPatch) -> None:
    session = MagicMock()
    session.post.return_value = _ok_response()
    client = _client(monkeypatch, session)

    client.generate(
        LlmRequest(
            prompt="hi",
            model="deepseek/deepseek-chat-v3.1",
            temperature=0.7,
            max_output_tokens=2048,
        )
    )

    payload = session.post.call_args.kwargs["json"]
    assert payload["model"] == "deepseek/deepseek-chat-v3.1"
    assert payload["temperature"] == 0.7
    assert payload["max_tokens"] == 2048
    assert payload["messages"][-1] == {"role": "user", "content": "hi"}


def test_json_mode_sets_response_format_and_injects_schema(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    session = MagicMock()
    session.post.return_value = _ok_response(content='{"ok":true}')
    client = _client(monkeypatch, session)

    schema = {"type": "object", "properties": {"ok": {"type": "boolean"}}}
    client.generate(
        LlmRequest(
            prompt="produce json",
            response_mime_type="application/json",
            response_json_schema=schema,
        )
    )

    payload = session.post.call_args.kwargs["json"]
    assert payload["response_format"] == {"type": "json_object"}
    # System message carries the schema (string form) so the model has
    # structure to comply with under json_object mode.
    system_msgs = [m for m in payload["messages"] if m["role"] == "system"]
    assert system_msgs, "expected an injected system message in JSON mode"
    assert '"properties"' in system_msgs[0]["content"]


# -------- model name normalisation --------


def test_strip_openrouter_prefix() -> None:
    assert (
        _strip_openrouter_prefix("openrouter/moonshotai/kimi-k2-0905")
        == "moonshotai/kimi-k2-0905"
    )
    assert (
        _strip_openrouter_prefix("moonshotai/kimi-k2-0905")
        == "moonshotai/kimi-k2-0905"
    )


def test_openclaw_style_model_name_is_normalised(monkeypatch: pytest.MonkeyPatch) -> None:
    session = MagicMock()
    session.post.return_value = _ok_response()
    client = _client(
        monkeypatch, session, model="openrouter/moonshotai/kimi-k2-0905"
    )

    client.generate(LlmRequest(prompt="hi"))

    payload = session.post.call_args.kwargs["json"]
    assert payload["model"] == "moonshotai/kimi-k2-0905"


# -------- error paths --------


def test_missing_api_key_raises(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("OPENROUTER_API_KEY", raising=False)
    with pytest.raises(LlmError) as exc_info:
        OpenRouterClient(_cfg())
    assert exc_info.value.error_code == "missing_api_key"


def test_http_401_maps_to_invalid_api_key(monkeypatch: pytest.MonkeyPatch) -> None:
    session = MagicMock()
    session.post.return_value = _err_response(401, '{"error":"bad key"}')
    client = _client(monkeypatch, session)

    with pytest.raises(LlmError) as exc_info:
        client.generate(LlmRequest(prompt="hi"))
    assert exc_info.value.error_code == "invalid_api_key"


def test_http_429_maps_to_rate_limited(monkeypatch: pytest.MonkeyPatch) -> None:
    session = MagicMock()
    session.post.return_value = _err_response(429, "rate-limited")
    client = _client(monkeypatch, session)

    with pytest.raises(LlmError) as exc_info:
        client.generate(LlmRequest(prompt="hi"))
    assert exc_info.value.error_code == "rate_limited"


def test_http_500_maps_to_llm_call_failed(monkeypatch: pytest.MonkeyPatch) -> None:
    session = MagicMock()
    session.post.return_value = _err_response(500, "upstream down")
    client = _client(monkeypatch, session)

    with pytest.raises(LlmError) as exc_info:
        client.generate(LlmRequest(prompt="hi"))
    assert exc_info.value.error_code == "llm_call_failed"


def test_network_exception_maps_to_network_error(monkeypatch: pytest.MonkeyPatch) -> None:
    session = MagicMock()
    session.post.side_effect = requests.ConnectionError("dns failed")
    client = _client(monkeypatch, session)

    with pytest.raises(LlmError) as exc_info:
        client.generate(LlmRequest(prompt="hi"))
    assert exc_info.value.error_code == "network_error"


def test_empty_choices_raises_empty_response(monkeypatch: pytest.MonkeyPatch) -> None:
    session = MagicMock()
    resp = MagicMock(spec=requests.Response)
    resp.status_code = 200
    resp.json.return_value = {"choices": []}
    resp.text = '{"choices": []}'
    session.post.return_value = resp
    client = _client(monkeypatch, session)

    with pytest.raises(LlmError) as exc_info:
        client.generate(LlmRequest(prompt="hi"))
    assert exc_info.value.error_code == "empty_response"


# -------- content-block tolerance --------


def test_content_blocks_are_concatenated(monkeypatch: pytest.MonkeyPatch) -> None:
    """Some OpenRouter upstreams stream content as a list of typed blocks
    (Anthropic-shaped). The adapter should join the text parts."""

    blocks = [
        {"type": "text", "text": '{"hello":'},
        {"type": "text", "text": " \"world\"}"},
    ]
    session = MagicMock()
    session.post.return_value = _ok_response(content=blocks)
    client = _client(monkeypatch, session)

    resp = client.generate(LlmRequest(prompt="hi"))

    assert resp.text == '{"hello": "world"}'


# -------- token usage fallback --------


def test_extract_usage_estimates_when_missing() -> None:
    p, o, t = _extract_usage(None, "0123456789" * 10, "abcdef" * 10)
    assert p > 0 and o > 0 and t == p + o


def test_extract_usage_handles_partial_counts() -> None:
    p, o, t = _extract_usage({"prompt_tokens": 10, "completion_tokens": 20}, "x", "y")
    assert (p, o, t) == (10, 20, 30)


# -------- factory --------


def test_factory_returns_openrouter_client(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("OPENROUTER_API_KEY", "sk-or-v1-test")
    client = make_llm_client(_cfg())
    assert isinstance(client, OpenRouterClient)


# -------- system instruction composition --------


def test_user_system_instruction_is_kept_alongside_schema() -> None:
    req = LlmRequest(
        prompt="hi",
        system_instruction="be terse",
        response_mime_type="application/json",
        response_json_schema={"type": "object"},
    )
    messages = _build_messages(req)
    system = [m for m in messages if m["role"] == "system"]
    assert system, "expected at least one system message"
    assert "be terse" in system[0]["content"]
    assert "Schema:" in system[0]["content"]


def test_extract_text_falls_back_to_legacy_text_field() -> None:
    """Some proxies omit message.content and stream `text` on the choice
    directly. Adapter accepts both."""

    body = {"choices": [{"text": "legacy"}]}
    assert _extract_text(body) == "legacy"
