"""Request/response data models for the prompt-compress API."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional


@dataclass
class CompressionSettings:
    """Compression tuning knobs."""

    aggressiveness: float = 0.5
    target_model: str = "gpt-4"


@dataclass
class CompressRequest:
    """Request body for POST /v1/compress."""

    input: str
    model: str = "scorer-v0.1"
    compression_settings: CompressionSettings = field(default_factory=CompressionSettings)
    session_id: Optional[str] = None
    agent: Optional[str] = None


@dataclass
class CompressResponse:
    """Response body from POST /v1/compress."""

    output: str
    output_tokens: int
    original_input_tokens: int
    compression_ratio: float


@dataclass
class CompressPresetResponse:
    """Response body from POST /v1/compress/preset/{name}."""

    preset: str
    output: str
    output_tokens: int
    original_input_tokens: int
    compression_ratio: float


@dataclass
class CompressDetectResponse:
    """Response body from POST /v1/compress/detect."""

    detected_preset: str
    output: str
    output_tokens: int
    original_input_tokens: int
    compression_ratio: float


@dataclass
class MetricsEntry:
    """Single session/agent metrics entry."""

    session_id: str
    agent: Optional[str] = None
    total_compressions: int = 0
    total_original_tokens: int = 0
    total_output_tokens: int = 0
    total_savings: int = 0
    avg_compression_ratio: float = 1.0


@dataclass
class MetricsResponse:
    """Response body from GET /v1/metrics."""

    sessions: list[MetricsEntry]
    total_compressions: int
    total_original_tokens: int
    total_output_tokens: int
    total_savings: int
    overall_compression_ratio: float
