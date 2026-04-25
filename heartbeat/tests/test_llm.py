"""Tests for the LLM adapter layer (E.2).

Strategy: every test mocks the underlying ``google.genai.Client`` so we
never make a real Gemini call. The adapter's job is to translate between
``LlmRequest``/``LlmResponse`` and the SDK; that translation is what we
verify here.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any
from unittest.mock import MagicMock

import pytest

from heartbeat.config import LlmConfig
from heartbeat.llm import (
    LlmClient,
    LlmError,
    LlmRequest,
    LlmResponse,
    make_llm_client,
)
from heartbeat.llm.gemini import (
    GeminiClient,
    _coerce_int,
    _estimate_tokens,
    _extract_usage,
)


@dataclass
class _FakeUsage:
    prompt_token_count: int | None = 100
    candidates_token_count: int | None = 50
    total_token_count: int | None = 150


@dataclass
class _FakeResponse:
    text: str
    usage_metadata: Any


def _fake_client(text: str = "hello", usage: Any | None = None) -> MagicMock:
    """Build a fake genai.Client whose models.generate_content returns a
    canned response."""

    if usage is None:
        usage = _FakeUsage()
    fake = MagicMock()
    fake.models.generate_content.return_value = _FakeResponse(text=text, usage_metadata=usage)
    return fake


def _cfg(provider: str = "gemini") -> LlmConfig:
    return LlmConfig(provider=provider, model="gemini-2.5-pro", temperature=0.2)


# -------- LlmClient protocol surface --------


def test_gemini_client_implements_protocol() -> None:
    """Static-typing-style check: a GeminiClient instance satisfies the
    ``LlmClient`` protocol."""
    client: LlmClient = GeminiClient(_cfg(), _client=_fake_client())
    assert hasattr(client, "generate")


# -------- happy path --------


def test_generate_returns_text_and_tokens() -> None:
    fake = _fake_client(text="reply!", usage=_FakeUsage(100, 50, 150))
    client = GeminiClient(_cfg(), _client=fake)

    resp = client.generate(LlmRequest(prompt="hi", system_instruction="be terse"))

    assert isinstance(resp, LlmResponse)
    assert resp.text == "reply!"
    assert resp.tokens_used == 150
    assert resp.prompt_tokens == 100
    assert resp.output_tokens == 50
    assert resp.model == "gemini-2.5-pro"


def test_generate_passes_config_to_sdk() -> None:
    fake = _fake_client()
    client = GeminiClient(_cfg(), _client=fake)

    client.generate(
        LlmRequest(
            prompt="hi",
            system_instruction="be terse",
            model="gemini-2.5-pro",
            temperature=0.7,
            max_output_tokens=2048,
        )
    )

    # Inspect the call passed to the SDK.
    fake.models.generate_content.assert_called_once()
    kwargs = fake.models.generate_content.call_args.kwargs
    assert kwargs["model"] == "gemini-2.5-pro"
    assert kwargs["contents"] == "hi"
    cfg = kwargs["config"]
    # GenerateContentConfig is a pydantic model — fields are direct attrs.
    assert cfg.system_instruction == "be terse"
    assert cfg.temperature == 0.7
    assert cfg.max_output_tokens == 2048


# -------- error paths --------


def test_generate_wraps_sdk_exception_as_llm_error() -> None:
    fake = MagicMock()
    fake.models.generate_content.side_effect = RuntimeError("rate limited")
    client = GeminiClient(_cfg(), _client=fake)

    with pytest.raises(LlmError) as exc_info:
        client.generate(LlmRequest(prompt="hi"))

    assert exc_info.value.error_code == "llm_call_failed"
    assert "rate limited" in str(exc_info.value)


def test_generate_raises_on_none_text() -> None:
    fake = _fake_client(text="", usage=_FakeUsage())
    fake.models.generate_content.return_value.text = None
    client = GeminiClient(_cfg(), _client=fake)

    with pytest.raises(LlmError) as exc_info:
        client.generate(LlmRequest(prompt="hi"))

    assert exc_info.value.error_code == "empty_response"


def test_generate_raises_when_text_accessor_throws() -> None:
    fake = MagicMock()
    bad_resp = MagicMock()
    type(bad_resp).text = property(
        lambda self: (_ for _ in ()).throw(ValueError("blocked by safety filter"))
    )
    bad_resp.usage_metadata = _FakeUsage()
    fake.models.generate_content.return_value = bad_resp
    client = GeminiClient(_cfg(), _client=fake)

    with pytest.raises(LlmError) as exc_info:
        client.generate(LlmRequest(prompt="hi"))

    assert exc_info.value.error_code == "empty_response"
    assert "blocked by safety filter" in str(exc_info.value)


def test_constructor_raises_when_api_key_missing(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("GEMINI_API_KEY", raising=False)
    monkeypatch.delenv("GOOGLE_API_KEY", raising=False)
    with pytest.raises(LlmError) as exc_info:
        # No _client= seam, so the real env-key path runs.
        GeminiClient(_cfg())
    assert exc_info.value.error_code == "missing_api_key"


# -------- token-count fallbacks --------


def test_extract_usage_returns_estimate_when_metadata_missing() -> None:
    p, o, t = _extract_usage(None, "0123456789" * 10, "abcdef" * 10)
    assert p > 0 and o > 0 and t == p + o


def test_extract_usage_handles_missing_total() -> None:
    usage = _FakeUsage(prompt_token_count=10, candidates_token_count=20, total_token_count=0)
    p, o, t = _extract_usage(usage, "x", "y")
    assert (p, o, t) == (10, 20, 30)


def test_extract_usage_handles_all_zero() -> None:
    usage = _FakeUsage(0, 0, 0)
    p, o, t = _extract_usage(usage, "abcd" * 5, "efgh" * 5)
    assert t == p + o
    assert p > 0 and o > 0


def test_coerce_int_handles_garbage() -> None:
    assert _coerce_int(None) == 0
    assert _coerce_int("not-a-number") == 0
    assert _coerce_int("42") == 42
    assert _coerce_int(42) == 42


def test_estimate_tokens_floors_at_one() -> None:
    p, o, t = _estimate_tokens("", "")
    assert p == 1 and o == 1 and t == 2


# -------- factory --------


def test_factory_returns_gemini_client_for_gemini_provider() -> None:
    # Bypass real SDK: monkeypatch the local import GeminiClient does.
    # Easier: provide a fake API key, then assert the type.
    import os

    os.environ["GEMINI_API_KEY"] = "test-key-not-real"
    try:
        client = make_llm_client(_cfg("gemini"))
        assert isinstance(client, GeminiClient)
    finally:
        del os.environ["GEMINI_API_KEY"]


def test_factory_raises_not_implemented_for_claude() -> None:
    with pytest.raises(NotImplementedError, match="claude"):
        make_llm_client(_cfg("claude"))


def test_factory_raises_value_error_for_unknown_provider() -> None:
    # We have to bypass the LlmConfig validator (which only allows
    # gemini/claude) by constructing the dataclass directly in a way that
    # mirrors a future provider. Use object.__setattr__ since LlmConfig is
    # frozen.
    cfg = LlmConfig(provider="gemini")
    object.__setattr__(cfg, "provider", "openai")
    with pytest.raises(ValueError, match="unknown llm.provider"):
        make_llm_client(cfg)
