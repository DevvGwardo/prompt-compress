"""Tests for prompt_compress client and models."""

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


class TestAsyncClientBasics:
    def test_init(self):
        client = AsyncPromptCompressor(base_url="http://localhost:9999", api_key="test-key")
        assert client.base_url == "http://localhost:9999"
