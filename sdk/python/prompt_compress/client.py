"""Sync and async clients for the prompt-compress HTTP API."""

from __future__ import annotations

from typing import Optional

import httpx

from .models import CompressRequest, CompressResponse, CompressionSettings

_DEFAULT_BASE_URL = "http://localhost:3000"


def _build_payload(req: CompressRequest) -> dict:
    return {
        "model": req.model,
        "input": req.input,
        "compression_settings": {
            "aggressiveness": req.compression_settings.aggressiveness,
            "target_model": req.compression_settings.target_model,
        },
    }


def _parse_response(data: dict) -> CompressResponse:
    return CompressResponse(
        output=data["output"],
        output_tokens=data["output_tokens"],
        original_input_tokens=data["original_input_tokens"],
        compression_ratio=data["compression_ratio"],
    )


def _headers(api_key: Optional[str]) -> dict[str, str]:
    if api_key:
        return {"Authorization": f"Bearer {api_key}"}
    return {}


class PromptCompressor:
    """Synchronous client for the prompt-compress API.

    Usage::

        client = PromptCompressor(base_url="http://localhost:3000", api_key="sk-...")
        result = client.compress("Your long prompt text here")
        print(result.output)
    """

    def __init__(
        self,
        base_url: str = _DEFAULT_BASE_URL,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self._headers = _headers(api_key)
        self._client = httpx.Client(base_url=self.base_url, headers=self._headers, timeout=timeout)

    def compress(
        self,
        text: str,
        *,
        model: str = "scorer-v0.1",
        aggressiveness: float = 0.5,
        target_model: str = "gpt-4",
    ) -> CompressResponse:
        """Compress a text prompt and return the result."""
        req = CompressRequest(
            input=text,
            model=model,
            compression_settings=CompressionSettings(
                aggressiveness=aggressiveness,
                target_model=target_model,
            ),
        )
        resp = self._client.post("/v1/compress", json=_build_payload(req))
        resp.raise_for_status()
        return _parse_response(resp.json())

    def close(self) -> None:
        self._client.close()

    def __enter__(self) -> "PromptCompressor":
        return self

    def __exit__(self, *exc) -> None:
        self.close()


class AsyncPromptCompressor:
    """Asynchronous client for the prompt-compress API.

    Usage::

        async with AsyncPromptCompressor(base_url="http://localhost:3000") as client:
            result = await client.compress("Your long prompt text here")
            print(result.output)
    """

    def __init__(
        self,
        base_url: str = _DEFAULT_BASE_URL,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self._headers = _headers(api_key)
        self._client = httpx.AsyncClient(
            base_url=self.base_url, headers=self._headers, timeout=timeout
        )

    async def compress(
        self,
        text: str,
        *,
        model: str = "scorer-v0.1",
        aggressiveness: float = 0.5,
        target_model: str = "gpt-4",
    ) -> CompressResponse:
        """Compress a text prompt and return the result."""
        req = CompressRequest(
            input=text,
            model=model,
            compression_settings=CompressionSettings(
                aggressiveness=aggressiveness,
                target_model=target_model,
            ),
        )
        resp = await self._client.post("/v1/compress", json=_build_payload(req))
        resp.raise_for_status()
        return _parse_response(resp.json())

    async def close(self) -> None:
        await self._client.aclose()

    async def __aenter__(self) -> "AsyncPromptCompressor":
        return self

    async def __aexit__(self, *exc) -> None:
        await self.close()
