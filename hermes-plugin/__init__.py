"""Hermes Plugin: prompt-compress integration.

Provides:
- /prompt-compress slash command: compress text using prompt-compress API
- `compress_prompt` tool: LLM-callable tool for on-demand compression
"""

from __future__ import annotations

import json
import logging
import os
from typing import Any

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Tool schema (OpenAI function-calling format)
# ---------------------------------------------------------------------------

COMPRESS_PROMPT_SCHEMA = {
    "name": "compress_prompt",
    "description": (
        "Compress a text prompt using the prompt-compress toolkit to reduce "
        "token count while preserving meaning. Scores token importance and "
        "removes low-value tokens. Use this to shrink system prompts, context "
        "windows, or any verbose text before sending to LLMs."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "text": {
                "type": "string",
                "description": "The input text to compress.",
            },
            "aggressiveness": {
                "type": "number",
                "description": (
                    "How aggressively to compress, from 0.0 (minimal) to 1.0 "
                    "(maximum). Default 0.5. Higher values remove more tokens."
                ),
                "minimum": 0.0,
                "maximum": 1.0,
                "default": 0.5,
            },
            "target_model": {
                "type": "string",
                "description": (
                    "The target LLM model for token counting (e.g., 'gpt-4', "
                    "'claude-3-opus'). Default 'gpt-4'."
                ),
                "default": "gpt-4",
            },
            "preset": {
                "type": "string",
                "enum": ["system", "context", "tools", "memory", None],
                "description": (
                    "Optional preset aggressiveness for common use-cases. "
                    "If set, overrides aggressiveness: system (0.3), context (0.5), "
                    "tools (0.2), memory (0.6)."
                ),
            },
        },
        "required": ["text"],
    },
}

# ---------------------------------------------------------------------------
# Preset aggressiveness mapping
# ---------------------------------------------------------------------------

_PRESET_AGGRESSIVENESS = {
    "system": 0.3,
    "context": 0.5,
    "tools": 0.2,
    "memory": 0.6,
}

# ---------------------------------------------------------------------------
# Helper: get compressor configuration
# ---------------------------------------------------------------------------


def _get_base_url() -> str:
    """Get the prompt-compress API base URL from env or default."""
    return os.environ.get("PROMPT_COMPRESS_BASE_URL", "http://localhost:3000")


def _get_api_key() -> str | None:
    """Get API key from env if set."""
    return os.environ.get("PROMPT_COMPRESS_API_KEY")


# ---------------------------------------------------------------------------
# Slash command handler
# ---------------------------------------------------------------------------

def _compress_via_sdk(text: str, aggressiveness: float, target_model: str) -> dict:
    """Call the prompt-compress Python SDK to compress text."""
    try:
        from prompt_compress import PromptCompressor
    except ImportError as e:
        raise RuntimeError(
            "prompt-compress SDK not installed. Run: pip install prompt-compress"
        ) from e

    base_url = _get_base_url()
    api_key = _get_api_key()

    client = PromptCompressor(base_url=base_url, api_key=api_key, timeout=30.0)
    try:
        resp = client.compress(
            text,
            aggressiveness=aggressiveness,
            target_model=target_model,
        )
        return {
            "output": resp.output,
            "output_tokens": resp.output_tokens,
            "original_input_tokens": resp.original_input_tokens,
            "compression_ratio": resp.compression_ratio,
        }
    finally:
        client.close()


def _parse_args(args: str) -> tuple[str, float, str]:
    """Parse slash command arguments into text, aggressiveness, target_model.

    Accepts flexible formats:
      "/prompt-compress <text>"
      "/prompt-compress <text> --aggressiveness 0.7"
      "/prompt-compress <text> --model gpt-4"
    """
    import shlex

    parts = shlex.split(args) if args else []
    text_parts = []
    aggressiveness: float = 0.5
    target_model: str = "gpt-4"

    i = 0
    while i < len(parts):
        part = parts[i]
        if part in ("-a", "--aggressiveness"):
            i += 1
            if i < len(parts):
                aggressiveness = float(parts[i])
        elif part in ("-m", "--model", "--target-model"):
            i += 1
            if i < len(parts):
                target_model = parts[i]
        else:
            text_parts.append(part)
        i += 1

    text = " ".join(text_parts).strip()
    if not text:
        raise ValueError("No text provided. Usage: /prompt-compress <text> [--aggressiveness N] [--model NAME]")

    return text, aggressiveness, target_model


def _handle_slash_command(args: str) -> str:
    """Handle /prompt-compress slash command."""
    try:
        text, aggressiveness, target_model = _parse_args(args)
    except ValueError as e:
        return f"Error: {e}"

    try:
        result = _compress_via_sdk(text, aggressiveness, target_model)
    except Exception as e:
        logger.exception("Compression failed")
        return f"Compression error: {e}"

    saved = result["original_input_tokens"] - result["output_tokens"]
    return (
        f"Compressed {result['original_input_tokens']} → {result['output_tokens']} tokens "
        f"(saved {saved}, ratio {result['compression_ratio']:.2%})\n\n"
        f"Output:\n{result['output']}"
    )


# ---------------------------------------------------------------------------
# Tool handler
# ---------------------------------------------------------------------------

def _handle_tool(args: dict) -> str:
    """Handle `compress_prompt` tool call from LLM."""
    text = args.get("text", "").strip()
    if not text:
        return json.dumps({"error": "text is required"})

    # Preset handling
    preset = args.get("preset")
    if preset:
        aggressiveness = _PRESET_AGGRESSIVENESS.get(preset, 0.5)
    else:
        aggressiveness = float(args.get("aggressiveness", 0.5))
        aggressiveness = max(0.0, min(1.0, aggressiveness))  # clamp

    target_model = args.get("target_model", "gpt-4")

    try:
        result = _compress_via_sdk(text, aggressiveness, target_model)
        return json.dumps(result)
    except Exception as e:
        logger.exception("Tool compression failed")
        return json.dumps({"error": str(e)})


def check_requirements() -> bool:
    """Return True if prompt-compress SDK is available."""
    try:
        import prompt_compress  # noqa: F401
        return True
    except ImportError:
        return False


# ---------------------------------------------------------------------------
# Plugin registration
# ---------------------------------------------------------------------------

def register(ctx) -> None:
    """Register plugin hooks, tools, and slash commands."""
    # Register the compress_prompt tool
    ctx.register_tool(
        name="compress_prompt",
        toolset="prompt-compress",
        schema=COMPRESS_PROMPT_SCHEMA,
        handler=_handle_tool,
        check_fn=check_requirements,
        emoji="🗜️",
        description="Compress text prompts using token importance scoring to reduce LLM costs.",
    )

    # Register the /prompt-compress slash command
    ctx.register_command(
        name="prompt-compress",
        handler=_handle_slash_command,
        description=(
            "Compress text using prompt-compress. "
            "Usage: /prompt-compress <text> [--aggressiveness N] [--model NAME]"
        ),
    )

    logger.info("prompt-compress plugin registered")
