"""Sync and async clients for the prompt-compress HTTP API."""

from __future__ import annotations

from typing import Optional

import httpx

from .models import (
    CompressRequest, CompressResponse, CompressPresetResponse, CompressDetectResponse,
    CompressionSettings, MetricsResponse, MetricsEntry,
)

_ = MetricsEntry  # re-export for callers

_DEFAULT_BASE_URL = "http://localhost:3000"


def _build_payload(req: CompressRequest) -> dict:
    payload = {
        "model": req.model,
        "input": req.input,
        "compression_settings": {
            "aggressiveness": req.compression_settings.aggressiveness,
            "target_model": req.compression_settings.target_model,
        },
    }
    if req.session_id:
        payload["session_id"] = req.session_id
    if req.agent:
        payload["agent"] = req.agent
    return payload


def _parse_response(data: dict) -> CompressResponse:
    return CompressResponse(
        output=data["output"],
        output_tokens=data["output_tokens"],
        original_input_tokens=data["original_input_tokens"],
        compression_ratio=data["compression_ratio"],
    )


def _parse_preset_response(data: dict) -> CompressPresetResponse:
    return CompressPresetResponse(
        preset=data["preset"],
        output=data["output"],
        output_tokens=data["output_tokens"],
        original_input_tokens=data["original_input_tokens"],
        compression_ratio=data["compression_ratio"],
    )


def _parse_detect_response(data: dict) -> CompressDetectResponse:
    return CompressDetectResponse(
        detected_preset=data["detected_preset"],
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
        session_id: Optional[str] = None,
        agent: Optional[str] = None,
    ) -> CompressResponse:
        """Compress a text prompt and return the result."""
        req = CompressRequest(
            input=text,
            model=model,
            compression_settings=CompressionSettings(
                aggressiveness=aggressiveness,
                target_model=target_model,
            ),
            session_id=session_id,
            agent=agent,
        )
        resp = self._client.post("/v1/compress", json=_build_payload(req))
        resp.raise_for_status()
        return _parse_response(resp.json())

    def compress_preset(
        self,
        text: str,
        preset: str,
        *,
        target_model: str = "gpt-4",
        session_id: Optional[str] = None,
        agent: Optional[str] = None,
    ) -> CompressPresetResponse:
        """Compress a text prompt using a named preset.

        Supported presets: ``system``, ``context``, ``tools``, ``memory``.
        """
        payload = {"input": text, "target_model": target_model}
        if session_id:
            payload["session_id"] = session_id
        if agent:
            payload["agent"] = agent
        resp = self._client.post(f"/v1/compress/preset/{preset}", json=payload)
        resp.raise_for_status()
        return _parse_preset_response(resp.json())

    def compress_detect(
        self,
        text: str,
        *,
        target_model: str = "gpt-4",
        session_id: Optional[str] = None,
        agent: Optional[str] = None,
    ) -> CompressDetectResponse:
        """Compress a text prompt with auto-detected preset.

        The server analyzes the content and picks the best preset
        (``system``, ``context``, ``tools``, or ``memory``).
        """
        payload = {"input": text, "target_model": target_model}
        if session_id:
            payload["session_id"] = session_id
        if agent:
            payload["agent"] = agent
        resp = self._client.post("/v1/compress/detect", json=payload)
        resp.raise_for_status()
        return _parse_detect_response(resp.json())

    def get_metrics(
        self,
        *,
        session_id: Optional[str] = None,
        agent: Optional[str] = None,
    ) -> MetricsResponse:
        """Fetch compression metrics from the server."""
        params: dict[str, str] = {}
        if session_id:
            params["session_id"] = session_id
        if agent:
            params["agent"] = agent
        resp = self._client.get("/v1/metrics", params=params)
        resp.raise_for_status()
        data = resp.json()
        return MetricsResponse(
            sessions=[
                MetricsEntry(
                    session_id=s["session_id"],
                    agent=s.get("agent"),
                    total_compressions=s["total_compressions"],
                    total_original_tokens=s["total_original_tokens"],
                    total_output_tokens=s["total_output_tokens"],
                    total_savings=s["total_savings"],
                    avg_compression_ratio=s["avg_compression_ratio"],
                )
                for s in data["sessions"]
            ],
            total_compressions=data["total_compressions"],
            total_original_tokens=data["total_original_tokens"],
            total_output_tokens=data["total_output_tokens"],
            total_savings=data["total_savings"],
            overall_compression_ratio=data["overall_compression_ratio"],
        )

    def health_check(self) -> bool:
        """Check whether the prompt-compress API server is healthy.

        Returns ``True`` if the server responds with a 200 status on the
        ``/health`` endpoint, ``False`` otherwise.
        """
        try:
            resp = self._client.get("/health")
            return resp.status_code == 200
        except Exception:
            return False

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
        session_id: Optional[str] = None,
        agent: Optional[str] = None,
    ) -> CompressResponse:
        """Compress a text prompt and return the result."""
        req = CompressRequest(
            input=text,
            model=model,
            compression_settings=CompressionSettings(
                aggressiveness=aggressiveness,
                target_model=target_model,
            ),
            session_id=session_id,
            agent=agent,
        )
        resp = await self._client.post("/v1/compress", json=_build_payload(req))
        resp.raise_for_status()
        return _parse_response(resp.json())

    async def compress_preset(
        self,
        text: str,
        preset: str,
        *,
        target_model: str = "gpt-4",
        session_id: Optional[str] = None,
        agent: Optional[str] = None,
    ) -> CompressPresetResponse:
        """Compress a text prompt using a named preset (async).

        Supported presets: ``system``, ``context``, ``tools``, ``memory``.
        """
        payload = {"input": text, "target_model": target_model}
        if session_id:
            payload["session_id"] = session_id
        if agent:
            payload["agent"] = agent
        resp = await self._client.post(f"/v1/compress/preset/{preset}", json=payload)
        resp.raise_for_status()
        return _parse_preset_response(resp.json())

    async def compress_detect(
        self,
        text: str,
        *,
        target_model: str = "gpt-4",
        session_id: Optional[str] = None,
        agent: Optional[str] = None,
    ) -> CompressDetectResponse:
        """Compress a text prompt with auto-detected preset (async).

        The server analyzes the content and picks the best preset
        (``system``, ``context``, ``tools``, or ``memory``).
        """
        payload = {"input": text, "target_model": target_model}
        if session_id:
            payload["session_id"] = session_id
        if agent:
            payload["agent"] = agent
        resp = await self._client.post("/v1/compress/detect", json=payload)
        resp.raise_for_status()
        return _parse_detect_response(resp.json())

    async def get_metrics(
        self,
        *,
        session_id: Optional[str] = None,
        agent: Optional[str] = None,
    ) -> MetricsResponse:
        """Fetch compression metrics from the server (async)."""
        params: dict[str, str] = {}
        if session_id:
            params["session_id"] = session_id
        if agent:
            params["agent"] = agent
        resp = await self._client.get("/v1/metrics", params=params)
        resp.raise_for_status()
        data = resp.json()
        return MetricsResponse(
            sessions=[
                MetricsEntry(
                    session_id=s["session_id"],
                    agent=s.get("agent"),
                    total_compressions=s["total_compressions"],
                    total_original_tokens=s["total_original_tokens"],
                    total_output_tokens=s["total_output_tokens"],
                    total_savings=s["total_savings"],
                    avg_compression_ratio=s["avg_compression_ratio"],
                )
                for s in data["sessions"]
            ],
            total_compressions=data["total_compressions"],
            total_original_tokens=data["total_original_tokens"],
            total_output_tokens=data["total_output_tokens"],
            total_savings=data["total_savings"],
            overall_compression_ratio=data["overall_compression_ratio"],
        )

    async def health_check(self) -> bool:
        """Check whether the prompt-compress API server is healthy (async).

        Returns ``True`` if the server responds with a 200 status on the
        ``/health`` endpoint, ``False`` otherwise.
        """
        try:
            resp = await self._client.get("/health")
            return resp.status_code == 200
        except Exception:
            return False

    async def close(self) -> None:
        await self._client.aclose()

    async def __aenter__(self) -> "AsyncPromptCompressor":
        return self

    async def __aexit__(self, *exc) -> None:
        await self.close()
