"""Tests for prompt_compress client and models."""

import httpx
import pytest

from prompt_compress import (
    AsyncPromptCompressor,
    CompressRequest,
    CompressResponse,
    CompressionSettings,
    PromptCompressor,
)
from prompt_compress.client import _build_payload, _parse_response, _parse_detect_response


class TestModels:
    def test_compression_settings_defaults(self):
        s = CompressionSettings()
        assert s.aggressiveness == 0.5
        assert s.target_model == "gpt-4"

    def test_compress_request_defaults(self):
        r = CompressRequest(input="hello")
        assert r.model == "scorer-v0.1"
        assert r.compression_settings.aggressiveness == 0.5

    def test_compress_request_session_and_agent(self):
        r = CompressRequest(input="hello", session_id="sess-abc", agent="hermes")
        assert r.session_id == "sess-abc"
        assert r.agent == "hermes"

    def test_compress_response_fields(self):
        r = CompressResponse(
            output="compressed",
            output_tokens=5,
            original_input_tokens=10,
            compression_ratio=0.5,
        )
        assert r.output == "compressed"
        assert r.output_tokens == 5
        assert r.original_input_tokens == 10
        assert r.compression_ratio == 0.5

    def test_compress_detect_response_fields(self):
        from prompt_compress import CompressDetectResponse
        r = CompressDetectResponse(
            detected_preset="system",
            output="compressed",
            output_tokens=5,
            original_input_tokens=10,
            compression_ratio=0.5,
        )
        assert r.detected_preset == "system"
        assert r.output == "compressed"
        assert r.output_tokens == 5


class TestPayloadHelpers:
    def test_build_payload(self):
        req = CompressRequest(
            input="test text",
            model="heuristic-v0.1",
            compression_settings=CompressionSettings(aggressiveness=0.3, target_model="gpt-3.5-turbo"),
        )
        payload = _build_payload(req)
        assert payload["model"] == "heuristic-v0.1"
        assert payload["input"] == "test text"
        assert payload["compression_settings"]["aggressiveness"] == 0.3
        assert payload["compression_settings"]["target_model"] == "gpt-3.5-turbo"

    def test_build_payload_with_session_and_agent(self):
        req = CompressRequest(
            input="test text",
            session_id="sess-123",
            agent="hermes-agent",
        )
        payload = _build_payload(req)
        assert payload["session_id"] == "sess-123"
        assert payload["agent"] == "hermes-agent"

    def test_build_payload_without_session_and_agent(self):
        req = CompressRequest(input="test text")
        payload = _build_payload(req)
        assert "session_id" not in payload
        assert "agent" not in payload

    def test_parse_response(self):
        data = {
            "output": "shortened",
            "output_tokens": 3,
            "original_input_tokens": 10,
            "compression_ratio": 0.3,
        }
        resp = _parse_response(data)
        assert resp.output == "shortened"
        assert resp.output_tokens == 3
        assert resp.original_input_tokens == 10
        assert resp.compression_ratio == 0.3

    def test_parse_detect_response(self):
        data = {
            "detected_preset": "memory",
            "output": "shortened",
            "output_tokens": 3,
            "original_input_tokens": 10,
            "compression_ratio": 0.3,
        }
        resp = _parse_detect_response(data)
        assert resp.detected_preset == "memory"
        assert resp.output == "shortened"
        assert resp.output_tokens == 3


class TestSyncClientBasics:
    def test_context_manager(self):
        """Ensure sync client works as context manager."""
        client = PromptCompressor(base_url="http://localhost:9999")
        assert client.base_url == "http://localhost:9999"
        client.close()

    def test_strips_trailing_slash(self):
        client = PromptCompressor(base_url="http://localhost:3000/")
        assert client.base_url == "http://localhost:3000"
        client.close()

    def test_health_check_ok(self, httpx_mock):
        """health_check returns True when /health responds 200."""
        httpx_mock.add_response(status_code=200, text="ok")
        client = PromptCompressor(base_url="http://localhost:3000")
        assert client.health_check() is True
        client.close()

    def test_health_check_fail(self, httpx_mock):
        """health_check returns False when /health responds non-200."""
        httpx_mock.add_response(status_code=503, text="unavailable")
        client = PromptCompressor(base_url="http://localhost:3000")
        assert client.health_check() is False
        client.close()

    def test_health_check_exception(self, httpx_mock):
        """health_check returns False on network errors."""
        httpx_mock.add_exception(httpx.ConnectError("Connection refused"))
        client = PromptCompressor(base_url="http://localhost:3000")
        assert client.health_check() is False
        client.close()


class TestAsyncClientBasics:
    def test_init(self):
        client = AsyncPromptCompressor(base_url="http://localhost:9999", api_key="test-key")
        assert client.base_url == "http://localhost:9999"

    async def test_async_health_check_ok(self, httpx_mock):
        """async health_check returns True when /health responds 200."""
        httpx_mock.add_response(status_code=200, text="ok")
        client = AsyncPromptCompressor(base_url="http://localhost:3000")
        assert await client.health_check() is True
        await client.close()

    async def test_async_health_check_fail(self, httpx_mock):
        """async health_check returns False when /health responds non-200."""
        httpx_mock.add_response(status_code=503, text="unavailable")
        client = AsyncPromptCompressor(base_url="http://localhost:3000")
        assert await client.health_check() is False
        await client.close()

    async def test_async_health_check_exception(self, httpx_mock):
        """async health_check returns False on network errors."""
        httpx_mock.add_exception(httpx.ConnectError("Connection refused"))
        client = AsyncPromptCompressor(base_url="http://localhost:3000")
        assert await client.health_check() is False
        await client.close()


class TestMetricsModels:
    def test_metrics_entry_fields(self):
        from prompt_compress import MetricsEntry
        e = MetricsEntry(
            session_id="sess-1",
            agent="hermes",
            total_compressions=10,
            total_original_tokens=1000,
            total_output_tokens=500,
            total_savings=500,
            avg_compression_ratio=0.5,
        )
        assert e.session_id == "sess-1"
        assert e.agent == "hermes"
        assert e.total_compressions == 10
        assert e.total_original_tokens == 1000
        assert e.total_output_tokens == 500
        assert e.total_savings == 500
        assert e.avg_compression_ratio == 0.5

    def test_metrics_response_fields(self):
        from prompt_compress import MetricsResponse, MetricsEntry
        r = MetricsResponse(
            sessions=[
                MetricsEntry(
                    session_id="sess-1",
                    agent="hermes",
                    total_compressions=1,
                    total_original_tokens=100,
                    total_output_tokens=50,
                    total_savings=50,
                    avg_compression_ratio=0.5,
                )
            ],
            total_compressions=1,
            total_original_tokens=100,
            total_output_tokens=50,
            total_savings=50,
            overall_compression_ratio=0.5,
        )
        assert r.sessions[0].session_id == "sess-1"
        assert r.sessions[0].agent == "hermes"
        assert r.total_compressions == 1
