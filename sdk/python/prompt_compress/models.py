"""Request/response data models for the prompt-compress API."""

from __future__ import annotations

from dataclasses import dataclass, field


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
