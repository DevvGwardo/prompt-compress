"""Tests for prompt_compress middleware."""

import pytest

from prompt_compress.middleware import (
    AsyncCompressMiddleware,
    CompressMiddleware,
    _extract_system_text,
    _replace_system_messages,
    _serialize_messages,
    _replace_old_context,
    _estimate_tokens,
)
from prompt_compress.models import CompressResponse, CompressPresetResponse


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

class FakeCompressor:
    """Fake sync compressor that always returns a predictable response."""

    def __init__(self, output="compressed", output_tokens=5, original_input_tokens=20):
        self.output = output
        self.output_tokens = output_tokens
        self.original_input_tokens = original_input_tokens
        self.calls = []

    def compress(self, text, *, aggressiveness=0.5, target_model="gpt-4", model="scorer-v0.1"):
        self.calls.append(("compress", text, aggressiveness, target_model, model))
        return CompressResponse(
            output=self.output,
            output_tokens=self.output_tokens,
            original_input_tokens=self.original_input_tokens,
            compression_ratio=(self.original_input_tokens - self.output_tokens)
            / self.original_input_tokens,
        )

    def compress_preset(self, text, preset, *, target_model="gpt-4"):
        self.calls.append(("compress_preset", text, preset, target_model))
        return CompressPresetResponse(
            preset=preset,
            output=self.output,
            output_tokens=self.output_tokens,
            original_input_tokens=self.original_input_tokens,
            compression_ratio=(self.original_input_tokens - self.output_tokens)
            / self.original_input_tokens,
        )


class FakeAsyncCompressor:
    """Fake async compressor."""

    def __init__(self, output="compressed", output_tokens=5, original_input_tokens=20):
        self.output = output
        self.output_tokens = output_tokens
        self.original_input_tokens = original_input_tokens
        self.calls = []

    async def compress(self, text, *, aggressiveness=0.5, target_model="gpt-4", model="scorer-v0.1"):
        self.calls.append(("compress", text, aggressiveness, target_model, model))
        return CompressResponse(
            output=self.output,
            output_tokens=self.output_tokens,
            original_input_tokens=self.original_input_tokens,
            compression_ratio=(self.original_input_tokens - self.output_tokens)
            / self.original_input_tokens,
        )

    async def compress_preset(self, text, preset, *, target_model="gpt-4"):
        self.calls.append(("compress_preset", text, preset, target_model))
        return CompressPresetResponse(
            preset=preset,
            output=self.output,
            output_tokens=self.output_tokens,
            original_input_tokens=self.original_input_tokens,
            compression_ratio=(self.original_input_tokens - self.output_tokens)
            / self.original_input_tokens,
        )


def fake_client(*args, **kwargs):
    return {"args": args, "kwargs": kwargs}


async def fake_async_client(*args, **kwargs):
    return {"args": args, "kwargs": kwargs}


# ---------------------------------------------------------------------------
# Unit tests for internal helpers
# ---------------------------------------------------------------------------

class TestExtractSystemText:
    def test_extracts_single_system(self):
        msgs = [{"role": "system", "content": "You are helpful."}]
        assert _extract_system_text(msgs) == "You are helpful."

    def test_extracts_multiple_system(self):
        msgs = [
            {"role": "system", "content": "First."},
            {"role": "user", "content": "Hi."},
            {"role": "system", "content": "Second."},
        ]
        assert _extract_system_text(msgs) == "First.\n\nSecond."

    def test_skips_non_system(self):
        msgs = [{"role": "user", "content": "Hello."}]
        assert _extract_system_text(msgs) == ""

    def test_multimodal_content(self):
        msgs = [
            {
                "role": "system",
                "content": [
                    {"type": "text", "text": "Part A"},
                    {"type": "image", "url": "http://example.com/img.png"},
                    {"type": "input_text", "text": "Part B"},
                ],
            }
        ]
        assert _extract_system_text(msgs) == "Part A Part B"

    def test_empty_content(self):
        assert _extract_system_text([]) == ""
        assert _extract_system_text([{"role": "system", "content": ""}]) == ""


class TestReplaceSystemMessages:
    def test_replaces_all_system(self):
        msgs = [
            {"role": "system", "content": "Old"},
            {"role": "user", "content": "Hi"},
            {"role": "system", "content": "Another"},
        ]
        result = _replace_system_messages(msgs, "New")
        assert len(result) == 2
        assert result[0] == {"role": "system", "content": "New"}
        assert result[1] == {"role": "user", "content": "Hi"}

    def test_no_system_messages(self):
        msgs = [{"role": "user", "content": "Hi"}]
        result = _replace_system_messages(msgs, "New")
        assert result == [{"role": "user", "content": "Hi"}]

    def test_deep_copy(self):
        msgs = [{"role": "user", "content": {"nested": "value"}}]
        result = _replace_system_messages(msgs, "New")
        assert result[0] is not msgs[0]


class TestSerializeMessages:
    def test_plain_text(self):
        msgs = [
            {"role": "user", "content": "Hello"},
            {"role": "assistant", "content": "Hi there"},
        ]
        assert _serialize_messages(msgs) == "USER: Hello\n\nASSISTANT: Hi there"

    def test_multimodal(self):
        msgs = [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "Look"},
                    {"type": "image", "url": "http://x.com/i.png"},
                ],
            }
        ]
        assert _serialize_messages(msgs) == "USER: Look"


class TestReplaceOldContext:
    def test_replaces_old_messages(self):
        msgs = [
            {"role": "user", "content": "Old 1"},
            {"role": "assistant", "content": "A1"},
            {"role": "user", "content": "Old 2"},
            {"role": "assistant", "content": "A2"},
            {"role": "user", "content": "New"},
            {"role": "assistant", "content": "A3"},
        ]
        result = _replace_old_context(msgs, "summary", protected_turns=1)
        assert len(result) == 3
        assert result[0]["role"] == "system"
        assert "summary" in result[0]["content"]
        assert result[1] == {"role": "user", "content": "New"}
        assert result[2] == {"role": "assistant", "content": "A3"}

    def test_not_enough_messages(self):
        msgs = [{"role": "user", "content": "Hi"}]
        result = _replace_old_context(msgs, "summary", protected_turns=1)
        assert result == msgs


class TestEstimateTokens:
    def test_empty(self):
        assert _estimate_tokens([]) == 0

    def test_plain_text(self):
        msgs = [{"role": "user", "content": "a" * 40}]
        # 40 chars / 4 = 10 tokens + 2 overhead = 12
        assert _estimate_tokens(msgs) == 12

    def test_multiple_messages(self):
        msgs = [
            {"role": "system", "content": "a" * 40},
            {"role": "user", "content": "b" * 40},
        ]
        # (40+40)/4 = 20 + 2*2 = 24
        assert _estimate_tokens(msgs) == 24

    def test_multimodal_content(self):
        msgs = [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "a" * 40},
                    {"type": "image", "url": "http://x.com/i.png"},
                ],
            }
        ]
        assert _estimate_tokens(msgs) == 12


# ---------------------------------------------------------------------------
# Sync middleware tests
# ---------------------------------------------------------------------------

class TestCompressMiddleware:
    def test_no_messages_passes_through(self):
        compressor = FakeCompressor()
        mw = CompressMiddleware(fake_client, compressor)
        result = mw(model="gpt-4")
        assert result["kwargs"].get("messages") is None
        assert mw.calls_made == 0

    def test_compress_system_prompt(self):
        compressor = FakeCompressor(output="compressed-sys", output_tokens=10, original_input_tokens=50)
        mw = CompressMiddleware(fake_client, compressor)
        msgs = [
            {"role": "system", "content": "X" * 200},
            {"role": "user", "content": "Hello"},
        ]
        result = mw(model="gpt-4", messages=msgs)
        out_msgs = result["kwargs"]["messages"]
        assert out_msgs[0]["role"] == "system"
        assert out_msgs[0]["content"] == "compressed-sys"
        assert out_msgs[1]["role"] == "user"
        assert mw.calls_made == 1
        assert mw.total_savings == 40

    def test_system_too_short_skipped(self):
        compressor = FakeCompressor()
        mw = CompressMiddleware(fake_client, compressor)
        msgs = [
            {"role": "system", "content": "Short"},
            {"role": "user", "content": "Hello"},
        ]
        result = mw(model="gpt-4", messages=msgs)
        assert result["kwargs"]["messages"][0]["content"] == "Short"
        assert mw.calls_made == 0

    def test_system_disabled(self):
        compressor = FakeCompressor()
        mw = CompressMiddleware(fake_client, compressor, compress_system=False)
        msgs = [
            {"role": "system", "content": "X" * 200},
            {"role": "user", "content": "Hello"},
        ]
        result = mw(model="gpt-4", messages=msgs)
        assert result["kwargs"]["messages"][0]["content"] == "X" * 200
        assert mw.calls_made == 0

    def test_context_compression(self):
        compressor = FakeCompressor(output="compressed-ctx", output_tokens=8, original_input_tokens=30)
        mw = CompressMiddleware(fake_client, compressor, compress_system=False)
        msgs = [
            {"role": "user", "content": "Old 1"},
            {"role": "assistant", "content": "A1"},
            {"role": "user", "content": "Old 2"},
            {"role": "assistant", "content": "A2"},
            {"role": "user", "content": "New"},
            {"role": "assistant", "content": "A3"},
        ]
        result = mw(model="gpt-4", messages=msgs)
        out_msgs = result["kwargs"]["messages"]
        # protected_turns=2 -> preserve last 4 messages (2 user+assistant pairs)
        assert len(out_msgs) == 5
        assert out_msgs[0]["role"] == "system"
        assert "compressed-ctx" in out_msgs[0]["content"]
        assert out_msgs[1]["content"] == "Old 2"
        assert out_msgs[2]["content"] == "A2"
        assert out_msgs[3]["content"] == "New"
        assert out_msgs[4]["content"] == "A3"
        assert mw.calls_made == 1
        assert mw.total_savings == 22
    def test_context_disabled(self):
        compressor = FakeCompressor()
        mw = CompressMiddleware(fake_client, compressor, compress_context=False)
        msgs = [
            {"role": "user", "content": "Old 1"},
            {"role": "assistant", "content": "A1"},
            {"role": "user", "content": "Old 2"},
            {"role": "assistant", "content": "A2"},
            {"role": "user", "content": "New"},
        ]
        result = mw(model="gpt-4", messages=msgs)
        assert len(result["kwargs"]["messages"]) == 5
        assert mw.calls_made == 0

    def test_error_warn_mode(self):
        class BrokenCompressor:
            def compress_preset(self, *a, **k):
                raise RuntimeError("boom")

            def compress(self, *a, **k):
                raise RuntimeError("boom")

        mw = CompressMiddleware(fake_client, BrokenCompressor(), on_error="warn")
        msgs = [
            {"role": "system", "content": "X" * 200},
            {"role": "user", "content": "Hello"},
        ]
        result = mw(model="gpt-4", messages=msgs)
        # Should fall back to original messages
        assert result["kwargs"]["messages"][0]["content"] == "X" * 200
        assert mw.calls_made == 0

    def test_error_raise_mode(self):
        class BrokenCompressor:
            def compress_preset(self, *a, **k):
                raise RuntimeError("boom")

            def compress(self, *a, **k):
                raise RuntimeError("boom")

        mw = CompressMiddleware(fake_client, BrokenCompressor(), on_error="raise")
        msgs = [
            {"role": "system", "content": "X" * 200},
            {"role": "user", "content": "Hello"},
        ]
        with pytest.raises(RuntimeError, match="boom"):
            mw(model="gpt-4", messages=msgs)

    def test_compression_ratio(self):
        compressor = FakeCompressor(output="x", output_tokens=5, original_input_tokens=25)
        mw = CompressMiddleware(fake_client, compressor)
        mw(model="gpt-4", messages=[{"role": "system", "content": "X" * 200}])
        assert mw.compression_ratio == 20 / 25

    def test_compression_ratio_no_calls(self):
        mw = CompressMiddleware(fake_client, FakeCompressor())
        assert mw.compression_ratio == 0.0

    def test_token_budget_no_enforcement_when_none(self):
        compressor = FakeCompressor()
        mw = CompressMiddleware(fake_client, compressor)
        msgs = [
            {"role": "system", "content": "X" * 200},
            {"role": "user", "content": "Hello"},
        ]
        result = mw(model="gpt-4", messages=msgs)
        # Should still compress system as normal
        assert result["kwargs"]["messages"][0]["content"] == "compressed"

    def test_token_budget_under_limit_no_change(self):
        compressor = FakeCompressor()
        mw = CompressMiddleware(fake_client, compressor, token_budget=500)
        msgs = [
            {"role": "system", "content": "X" * 200},
            {"role": "user", "content": "Hello"},
        ]
        result = mw(model="gpt-4", messages=msgs)
        out_msgs = result["kwargs"]["messages"]
        assert out_msgs[0]["content"] == "compressed"
        assert out_msgs[1]["content"] == "Hello"

    def test_token_budget_triggers_recompression(self):
        # Create a compressor that returns smaller output at higher aggressiveness
        class BudgetCompressor:
            def __init__(self):
                self.calls = []

            def compress_preset(self, text, preset, *, target_model="gpt-4"):
                self.calls.append(("compress_preset", preset))
                return CompressPresetResponse(
                    preset=preset,
                    output="compressed-sys",
                    output_tokens=10,
                    original_input_tokens=50,
                    compression_ratio=0.8,
                )

            def compress(self, text, *, aggressiveness=0.5, target_model="gpt-4", model="scorer-v0.1"):
                self.calls.append(("compress", aggressiveness))
                # At higher aggressiveness, return much smaller output
                if aggressiveness >= 0.7:
                    return CompressResponse(
                        output="tiny",
                        output_tokens=2,
                        original_input_tokens=30,
                        compression_ratio=0.93,
                    )
                return CompressResponse(
                    output="compressed-ctx",
                    output_tokens=20,
                    original_input_tokens=30,
                    compression_ratio=0.33,
                )

        compressor = BudgetCompressor()
        mw = CompressMiddleware(fake_client, compressor, token_budget=20, compress_system=False)
        # 6 messages -> 2 pairs protected = 4 protected, 2 old messages
        msgs = [
            {"role": "user", "content": "Old message one here"},
            {"role": "assistant", "content": "Assistant reply number one"},
            {"role": "user", "content": "Old message two here"},
            {"role": "assistant", "content": "Assistant reply number two"},
            {"role": "user", "content": "New user message here"},
            {"role": "assistant", "content": "New assistant reply here"},
        ]
        result = mw(model="gpt-4", messages=msgs)
        out_msgs = result["kwargs"]["messages"]
        # Should have re-compressed at higher aggressiveness
        assert any(call == ("compress", 0.7) for call in compressor.calls)
        assert out_msgs[0]["role"] == "system"
        assert "tiny" in out_msgs[0]["content"]

    def test_token_budget_drops_messages_when_recompression_insufficient(self):
        class NoBudgetCompressor:
            def compress_preset(self, text, preset, *, target_model="gpt-4"):
                return CompressPresetResponse(
                    preset=preset,
                    output="compressed-sys",
                    output_tokens=10,
                    original_input_tokens=50,
                    compression_ratio=0.8,
                )

            def compress(self, text, *, aggressiveness=0.5, target_model="gpt-4", model="scorer-v0.1"):
                return CompressResponse(
                    output="still-big",
                    output_tokens=25,
                    original_input_tokens=30,
                    compression_ratio=0.17,
                )

        compressor = NoBudgetCompressor()
        mw = CompressMiddleware(fake_client, compressor, token_budget=10, compress_system=False)
        msgs = [
            {"role": "user", "content": "First message content"},
            {"role": "assistant", "content": "Assistant reply one content"},
            {"role": "user", "content": "Second message content"},
            {"role": "assistant", "content": "Assistant reply two content"},
        ]
        result = mw(model="gpt-4", messages=msgs)
        out_msgs = result["kwargs"]["messages"]
        # Should drop oldest non-system messages until under budget
        assert len(out_msgs) < len(msgs)


# ---------------------------------------------------------------------------
# Async middleware tests
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestAsyncCompressMiddleware:
    async def test_no_messages_passes_through(self):
        compressor = FakeAsyncCompressor()
        mw = AsyncCompressMiddleware(fake_async_client, compressor)
        result = await mw(model="gpt-4")
        assert result["kwargs"].get("messages") is None
        assert mw.calls_made == 0

    async def test_compress_system_prompt(self):
        compressor = FakeAsyncCompressor(output="compressed-sys", output_tokens=10, original_input_tokens=50)
        mw = AsyncCompressMiddleware(fake_async_client, compressor)
        msgs = [
            {"role": "system", "content": "X" * 200},
            {"role": "user", "content": "Hello"},
        ]
        result = await mw(model="gpt-4", messages=msgs)
        out_msgs = result["kwargs"]["messages"]
        assert out_msgs[0]["content"] == "compressed-sys"
        assert mw.calls_made == 1
        assert mw.total_savings == 40

    async def test_compress_context(self):
        compressor = FakeAsyncCompressor(output="compressed-ctx", output_tokens=8, original_input_tokens=30)
        mw = AsyncCompressMiddleware(fake_async_client, compressor, compress_system=False)
        msgs = [
            {"role": "user", "content": "Old 1"},
            {"role": "assistant", "content": "A1"},
            {"role": "user", "content": "Old 2"},
            {"role": "assistant", "content": "A2"},
            {"role": "user", "content": "New"},
        ]
        result = await mw(model="gpt-4", messages=msgs)
        out_msgs = result["kwargs"]["messages"]
        # protected_turns=2 -> preserve last 4 messages (2 user+assistant pairs)
        assert len(out_msgs) == 5
        assert out_msgs[0]["role"] == "system"
        assert "compressed-ctx" in out_msgs[0]["content"]
        assert out_msgs[1]["content"] == "A1"
        assert out_msgs[2]["content"] == "Old 2"
        assert out_msgs[3]["content"] == "A2"
        assert out_msgs[4]["content"] == "New"
        assert mw.calls_made == 1

    async def test_error_warn_mode(self):
        class BrokenAsyncCompressor:
            async def compress_preset(self, *a, **k):
                raise RuntimeError("boom")

            async def compress(self, *a, **k):
                raise RuntimeError("boom")

        mw = AsyncCompressMiddleware(fake_async_client, BrokenAsyncCompressor(), on_error="warn")
        msgs = [
            {"role": "system", "content": "X" * 200},
            {"role": "user", "content": "Hello"},
        ]
        result = await mw(model="gpt-4", messages=msgs)
        assert result["kwargs"]["messages"][0]["content"] == "X" * 200
        assert mw.calls_made == 0

    async def test_token_budget_async_enforcement(self):
        class BudgetAsyncCompressor:
            def __init__(self):
                self.calls = []

            async def compress_preset(self, text, preset, *, target_model="gpt-4"):
                self.calls.append(("compress_preset", preset))
                return CompressPresetResponse(
                    preset=preset,
                    output="compressed-sys",
                    output_tokens=10,
                    original_input_tokens=50,
                    compression_ratio=0.8,
                )

            async def compress(self, text, *, aggressiveness=0.5, target_model="gpt-4", model="scorer-v0.1"):
                self.calls.append(("compress", aggressiveness))
                if aggressiveness >= 0.7:
                    return CompressResponse(
                        output="tiny",
                        output_tokens=2,
                        original_input_tokens=30,
                        compression_ratio=0.93,
                    )
                return CompressResponse(
                    output="compressed-ctx",
                    output_tokens=20,
                    original_input_tokens=30,
                    compression_ratio=0.33,
                )

        compressor = BudgetAsyncCompressor()
        mw = AsyncCompressMiddleware(fake_async_client, compressor, token_budget=20, compress_system=False)
        msgs = [
            {"role": "user", "content": "Old message one here"},
            {"role": "assistant", "content": "Assistant reply number one"},
            {"role": "user", "content": "Old message two here"},
            {"role": "assistant", "content": "Assistant reply number two"},
            {"role": "user", "content": "New user message here"},
            {"role": "assistant", "content": "New assistant reply here"},
        ]
        result = await mw(model="gpt-4", messages=msgs)
        out_msgs = result["kwargs"]["messages"]
        assert any(call == ("compress", 0.7) for call in compressor.calls)
        assert out_msgs[0]["role"] == "system"
        assert "tiny" in out_msgs[0]["content"]
