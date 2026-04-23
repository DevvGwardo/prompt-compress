"""Integration tests for prompt_compress client against a live compress-api server.

These tests require the compress-api server to be running on localhost:3000.
To start the server:
    cd crates/compress-api && cargo run

Or use the provided script:
    ./scripts/start-api-server.sh

The tests will be automatically skipped if the server is not reachable.
"""

import pytest
import httpx

from prompt_compress import PromptCompressor, AsyncPromptCompressor, CompressionSettings, CompressPresetResponse


# Skip all tests in this module if the API server is not reachable
def _server_is_available():
    """Check if the compress-api server is running and reachable."""
    try:
        with httpx.Client(timeout=1.0) as client:
            resp = client.get("http://localhost:3000/health")
            return resp.status_code == 200
    except Exception:
        return False


pytestmark = pytest.mark.skipif(
    not _server_is_available(),
    reason="compress-api server not running on localhost:3000. Start with: cd crates/compress-api && cargo run",
)


class TestLiveAPISync:
    """Tests against a live synchronous API."""

    def test_compress_basic(self):
        """Test basic compression returns valid response."""
        client = PromptCompressor()
        result = client.compress("This is a test prompt with some content.")
        assert result.output
        assert result.output_tokens > 0
        assert result.original_input_tokens > 0
        assert result.compression_ratio > 0
        assert result.compression_ratio <= 1.0
        client.close()

    def test_compress_aggressiveness(self):
        """Test that higher aggressiveness yields more compression."""
        client = PromptCompressor()
        text = "This is a longer prompt that will allow us to see the effects of compression. " * 5

        low_result = client.compress(text, aggressiveness=0.2)
        high_result = client.compress(text, aggressiveness=0.8)

        assert high_result.compression_ratio < low_result.compression_ratio
        assert high_result.output_tokens < low_result.output_tokens
        client.close()

    def test_compress_preserves_meaning(self):
        """Test that compression preserves core meaning for simple prompts."""
        client = PromptCompressor()
        text = "Analyze the following code and fix any security vulnerabilities."
        result = client.compress(text, aggressiveness=0.5, target_model="gpt-4")
        assert "analyze" in result.output.lower() or "code" in result.output.lower()
        client.close()

    def test_context_manager_works(self):
        """Test that sync client works as a context manager with real API."""
        with PromptCompressor() as client:
            result = client.compress("Hello world")
            assert result.output
            assert result.output_tokens > 0

    def test_custom_base_url(self):
        """Test using a custom base URL (still pointing to same server)."""
        client = PromptCompressor(base_url="http://localhost:3000")
        result = client.compress("Test")
        assert result.output
        client.close()

    def test_heuristic_model(self):
        """Test using the heuristic model."""
        client = PromptCompressor()
        result = client.compress("Some text here", model="heuristic-v0.1")
        assert result.output
        client.close()


class TestLiveAPIAsync:
    """Tests against a live asynchronous API."""

    @pytest.mark.asyncio
    async def test_async_compress_basic(self):
        """Test basic async compression."""
        async with AsyncPromptCompressor() as client:
            result = await client.compress("Async test prompt")
            assert result.output
            assert result.output_tokens > 0

    @pytest.mark.asyncio
    async def test_async_compress_aggressiveness(self):
        """Test async compression with different aggressiveness levels."""
        async with AsyncPromptCompressor() as client:
            # Use natural language text - uniform chars (e.g. "A"*500) don't compress
            text = "This is a longer prompt that will allow us to see the effects of compression. " * 5

            low = await client.compress(text, aggressiveness=0.2)
            high = await client.compress(text, aggressiveness=0.8)

            assert high.compression_ratio < low.compression_ratio

    @pytest.mark.asyncio
    async def test_async_context_manager(self):
        """Test async context manager."""
        async with AsyncPromptCompressor() as client:
            await client.compress("test")
            # Should not raise

    @pytest.mark.asyncio
    async def test_async_custom_model(self):
        """Test async with heuristic model."""
        async with AsyncPromptCompressor() as client:
            result = await client.compress("test", model="heuristic-v0.1")
            assert result.output


class TestLiveAPIResponseContent:
    """Tests for compressed output content quality."""

    def test_compression_ratio_bounds(self):
        """Test compression ratio is always between 0 and 1."""
        client = PromptCompressor()
        text = "This is a reasonably long prompt that should compress somewhat. " * 10
        result = client.compress(text, aggressiveness=0.5)
        assert 0.0 < result.compression_ratio <= 1.0
        client.close()

    def test_output_non_empty(self):
        """Test compressed output is never empty for non-empty input."""
        client = PromptCompressor()
        result = client.compress("Some input text")
        assert result.output.strip() != ""
        client.close()

    def test_token_counts_plausible(self):
        """Test token counts are reasonable (output <= input)."""
        client = PromptCompressor()
        text = "The quick brown fox jumps over the lazy dog."
        result = client.compress(text, aggressiveness=0.5)
        assert result.output_tokens <= result.original_input_tokens
        client.close()

    def test_compression_with_special_characters(self):
        """Test compression handles special characters."""
        client = PromptCompressor()
        text = "Hello! @#$%^&*()_+-=[]{}|;':\",./<>?"
        result = client.compress(text)
        assert result.output
        client.close()


class TestLiveAPIPresetSync:
    """Tests for preset endpoint (sync)."""

    def test_preset_system(self):
        """Test system preset compresses aggressively."""
        client = PromptCompressor()
        text = "This is a system prompt telling the agent to analyze code and fix bugs."
        result = client.compress_preset(text, "system")
        assert isinstance(result, CompressPresetResponse)
        assert result.preset == "system"
        assert result.output
        assert result.output_tokens > 0
        client.close()

    def test_preset_memory(self):
        """Test memory preset compresses more aggressively."""
        client = PromptCompressor()
        text = "Previous conversation memory: user asked about Python list comprehensions."
        result = client.compress_preset(text, "memory")
        assert result.preset == "memory"
        assert result.output
        client.close()

    def test_preset_invalid_returns_error(self):
        """Test invalid preset raises HTTP error."""
        client = PromptCompressor()
        with pytest.raises(httpx.HTTPStatusError):
            client.compress_preset("test", "invalid-preset")
        client.close()

    def test_preset_tools(self):
        """Test tools preset works."""
        client = PromptCompressor()
        text = "Tool definition for search_web: query string, returns JSON."
        result = client.compress_preset(text, "tools")
        assert result.preset == "tools"
        assert result.output
        client.close()


class TestLiveAPIPresetAsync:
    """Tests for preset endpoint (async)."""

    @pytest.mark.asyncio
    async def test_async_preset_context(self):
        """Test async context preset."""
        async with AsyncPromptCompressor() as client:
            result = await client.compress_preset("Some context here", "context")
            assert result.preset == "context"
            assert result.output

    @pytest.mark.asyncio
    async def test_async_preset_invalid_raises(self):
        """Test async invalid preset raises."""
        async with AsyncPromptCompressor() as client:
            with pytest.raises(httpx.HTTPStatusError):
                await client.compress_preset("test", "nope")


class TestLiveAPIDetectSync:
    """Tests for auto-detect endpoint (sync)."""

    def test_detect_system_prompt(self):
        """Test auto-detect picks system preset for instruction text."""
        client = PromptCompressor()
        text = "You are a helpful assistant. Your task is to write clean code. You must follow PEP 8."
        result = client.compress_detect(text)
        assert result.detected_preset == "system"
        assert result.output
        client.close()

    def test_detect_tools_prompt(self):
        """Test auto-detect picks tools preset for JSON schema text."""
        client = PromptCompressor()
        text = '{"type": "function", "name": "get_weather", "parameters": {"properties": {"city": {"type": "string"}}, "required": ["city"]}}'
        result = client.compress_detect(text)
        assert result.detected_preset == "tools"
        assert result.output
        client.close()

    def test_detect_memory_prompt(self):
        """Test auto-detect picks memory preset for recall text."""
        client = PromptCompressor()
        text = "Earlier we discussed deployment. You said Kubernetes. I said Docker Compose."
        result = client.compress_detect(text)
        assert result.detected_preset == "memory"
        assert result.output
        client.close()

    def test_detect_context_fallback(self):
        """Test auto-detect falls back to context for generic text."""
        client = PromptCompressor()
        text = "The quick brown fox jumps over the lazy dog."
        result = client.compress_detect(text)
        assert result.detected_preset == "context"
        assert result.output
        client.close()


class TestLiveAPIDetectAsync:
    """Tests for auto-detect endpoint (async)."""

    @pytest.mark.asyncio
    async def test_async_detect_system(self):
        """Test async auto-detect picks system preset."""
        async with AsyncPromptCompressor() as client:
            text = "You are an expert. Your role is to review code. You should be thorough."
            result = await client.compress_detect(text)
            assert result.detected_preset == "system"
            assert result.output
