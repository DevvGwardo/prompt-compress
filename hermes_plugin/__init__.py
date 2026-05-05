"""Hermes Plugin: prompt-compress integration via CLI binary.

Provides:
- /prompt-compress slash command: compress text using prompt-compress binary
- compress_prompt tool: LLM-callable tool for on-demand compression
- pre_llm_call hook: auto-compresses system prompts & old context

Uses the compress CLI binary directly — no Python SDK or HTTP API needed.
"""

from __future__ import annotations

import json
import logging
import os
import shutil
import subprocess
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

COMPRESS_PRESETS = {
    "system": 0.3,   # System/developer prompts — keep core instructions
    "context": 0.5,  # Accumulated context — balance detail vs size
    "tools": 0.2,    # Tool definitions — preserve schemas and structure
    "memory": 0.6,   # Memory/recall entries — aggressive, key facts only
}

COMPRESS_BIN_CANDIDATES = [
    # User env
    os.environ.get("PROMPT_COMPRESS_BIN"),
    # Dev build in the prompt-compress repo
    str(Path.home() / "prompt-compress" / "target" / "release" / "compress"),
    # Expected on PATH after install
    shutil.which("compress"),
]

# ---------------------------------------------------------------------------
# Tool schema (OpenAI function-calling format)
# ---------------------------------------------------------------------------

COMPRESS_PROMPT_SCHEMA = {
    "name": "compress_prompt",
    "description": (
        "Compress a text prompt using token importance scoring to reduce "
        "token count while preserving meaning. Removes low-value tokens. "
        "Use this to shrink system prompts, context windows, or verbose text."
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
                "enum": ["system", "context", "tools", "memory"],
                "description": (
                    "Optional preset aggressiveness for common use-cases. "
                    "If set, overrides aggressiveness: "
                    "system (0.3), context (0.5), tools (0.2), memory (0.6)."
                ),
            },
            "scorer_mode": {
                "type": "string",
                "enum": ["standard", "agent-aware"],
                "description": (
                    "Scoring mode: 'standard' for general-purpose text, "
                    "'agent-aware' optimized for agent instructions and tool calls."
                ),
                "default": "agent-aware",
            },
        },
        "required": ["text"],
    },
}


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _get_compress_bin() -> str | None:
    """Find the compress binary."""
    for candidate in COMPRESS_BIN_CANDIDATES:
        if candidate and os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            return candidate
    return None


def _compress_via_cli(
    text: str,
    aggressiveness: float,
    target_model: str,
    scorer_mode: str = "agent-aware",
) -> dict:
    """Call the compress CLI binary to compress text."""
    bin_path = _get_compress_bin()
    if not bin_path:
        raise RuntimeError(
            "compress binary not found. Build it:\n"
            "  cd ~/prompt-compress && cargo build --release"
        )

    args = [
        bin_path,
        "--aggressiveness", str(aggressiveness),
        "--target-model", target_model,
        "--scorer-mode", scorer_mode,
        "--format", "json",
    ]

    try:
        result = subprocess.run(
            args,
            input=text,
            capture_output=True,
            text=True,
            timeout=30,
        )
    except subprocess.TimeoutExpired:
        raise RuntimeError("compress timed out after 30s")

    if result.returncode != 0:
        stderr = result.stderr.strip() or "unknown error"
        raise RuntimeError(f"compress failed: {stderr}")

    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError as e:
        raise RuntimeError(f"compress output parse error: {e}")

    return data


def _parse_args(args: str) -> tuple[str, float, str, str]:
    """Parse slash command arguments.

    /prompt-compress <text> [--aggressiveness 0.7] [--model gpt-4]
                     [--scorer-mode standard|agent-aware]
    """
    import shlex

    parts = shlex.split(args) if args else []
    text_parts = []
    aggressiveness: float = 0.5
    target_model: str = "gpt-4"
    scorer_mode: str = "agent-aware"

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
        elif part in ("-s", "--scorer-mode"):
            i += 1
            if i < len(parts):
                scorer_mode = parts[i]
        else:
            text_parts.append(part)
        i += 1

    text = " ".join(text_parts).strip()
    if not text:
        raise ValueError(
            "No text provided. Usage: /prompt-compress <text> "
            "[--aggressiveness N] [--model NAME] [--scorer-mode standard|agent-aware]"
        )

    return text, aggressiveness, target_model, scorer_mode


def _format_result(result: dict) -> str:
    """Format compression result for display."""
    saved = result["original_input_tokens"] - result["output_tokens"]
    ratio = result["compression_ratio"] * 100
    return (
        f"Compressed {result['original_input_tokens']} -> {result['output_tokens']} tokens "
        f"(saved {saved}, ratio {ratio:.1f}%)\n\n"
        f"Output:\n{result['output']}"
    )


# ---------------------------------------------------------------------------
# Slash command handler
# ---------------------------------------------------------------------------

def _handle_slash_command(args: str) -> str:
    """Handle /prompt-compress slash command."""
    try:
        text, aggressiveness, target_model, scorer_mode = _parse_args(args)
    except ValueError as e:
        return f"Error: {e}"

    try:
        result = _compress_via_cli(text, aggressiveness, target_model, scorer_mode)
    except Exception as e:
        logger.exception("Slash command compression failed")
        return f"Compression error: {e}"

    return _format_result(result)


# ---------------------------------------------------------------------------
# Tool handler
# ---------------------------------------------------------------------------

def _handle_tool(args: dict) -> str:
    """Handle compress_prompt tool call from LLM."""
    text = args.get("text", "").strip()
    if not text:
        return json.dumps({"error": "text is required"})

    # Preset overrides explicit aggressiveness
    preset = args.get("preset")
    if preset:
        aggressiveness = COMPRESS_PRESETS.get(preset, 0.5)
    else:
        aggressiveness = float(args.get("aggressiveness", 0.5))
        aggressiveness = max(0.0, min(1.0, aggressiveness))

    target_model = args.get("target_model", "gpt-4")
    scorer_mode = args.get("scorer_mode", "agent-aware")

    try:
        result = _compress_via_cli(text, aggressiveness, target_model, scorer_mode)
        return json.dumps(result)
    except Exception as e:
        logger.exception("Tool compression failed")
        return json.dumps({"error": str(e)})


# ---------------------------------------------------------------------------
# Requirements check
# ---------------------------------------------------------------------------

def check_requirements() -> bool:
    """Return True if the compress binary is available."""
    return _get_compress_bin() is not None


# ---------------------------------------------------------------------------
# Hook: on_session_start
# ---------------------------------------------------------------------------

def _on_session_start(session_id: str, model: str = "", platform: str = "") -> None:
    """Verify the compress binary is available on session start."""
    if not _get_compress_bin():
        logger.warning(
            "prompt-compress binary not found. Install from: "
            "https://github.com/DevvGwardo/prompt-compress"
        )
    else:
        logger.info("prompt-compress ready for session %s", session_id)


# ---------------------------------------------------------------------------
# Hook: pre_llm_call
# ---------------------------------------------------------------------------

def _pre_llm_call(
    session_id: str,
    user_message: str,
    conversation_history: list,
    is_first_turn: bool = False,
    model: str = "",
    platform: str = "",
    sender_id: str = "",
) -> str | dict | None:
    """Compress system prompts and old conversation history before LLM calls.

    1. System-prompt compression — compresses role=system messages
       using the 'system' preset (aggressiveness 0.3).
    2. Context-window compression — preserves last 2 turns,
       compresses everything before that.

    Returns dict with optional system_prompt and/or context keys.
    """
    result: dict[str, str] = {}

    # -- 1. System-prompt compression --
    system_text = _extract_system_prompts(conversation_history)
    if system_text and len(system_text) > 150:
        try:
            compressed = _compress_via_cli(
                system_text,
                aggressiveness=COMPRESS_PRESETS["system"],
                target_model=model or "gpt-4",
                scorer_mode="agent-aware",
            )
            savings = compressed["original_input_tokens"] - compressed["output_tokens"]
            if savings >= 5:
                result["system_prompt"] = compressed["output"]
                logger.info(
                    "Compressed system prompt: %d -> %d tokens (saved %d)",
                    compressed["original_input_tokens"],
                    compressed["output_tokens"],
                    savings,
                )
        except Exception as exc:
            logger.warning("System prompt compression failed: %s", exc)

    # -- 2. Context-window compression --
    if not is_first_turn and len(conversation_history) >= 4:
        try:
            PROTECTED_TURNS = 2
            protect_count = PROTECTED_TURNS * 2
            cutoff = max(0, len(conversation_history) - protect_count)
            old_history = conversation_history[:cutoff]

            if old_history:
                serialized = _serialize_conversation(old_history)

                if len(old_history) > 10:
                    aggr = 0.6
                elif len(old_history) > 5:
                    aggr = 0.5
                else:
                    aggr = 0.4

                compressed = _compress_via_cli(
                    serialized,
                    aggressiveness=aggr,
                    target_model=model or "gpt-4",
                    scorer_mode="standard",
                )

                savings = compressed["original_input_tokens"] - compressed["output_tokens"]
                if savings >= 10:
                    result["context"] = (
                        f"[Compressed context from {len(old_history)} earlier turn(s) — "
                        f"{compressed['original_input_tokens']} -> {compressed['output_tokens']} tokens "
                        f"saved {savings}]:\n{compressed['output']}"
                    )
                    logger.info(
                        "Compressed %d old turns: %d -> %d tokens (saved %d)",
                        len(old_history),
                        compressed["original_input_tokens"],
                        compressed["output_tokens"],
                        savings,
                    )
        except Exception as exc:
            logger.warning("pre_llm_call context compression failed: %s", exc)

    return result if result else None


def _extract_system_prompts(messages: list) -> str:
    """Extract and concatenate all system prompt messages from history."""
    parts = []
    for msg in messages:
        if msg.get("role") == "system":
            content = msg.get("content", "")
            if isinstance(content, list):
                text_parts = []
                for block in content:
                    if isinstance(block, dict) and block.get("type") in ("text", "input_text"):
                        text_parts.append(str(block.get("text", "")))
                content = " ".join(text_parts)
            else:
                content = str(content)
            if content.strip():
                parts.append(content.strip())
    return "\n\n".join(parts)


def _serialize_conversation(messages: list) -> str:
    """Serialize chat messages into plain text for compression."""
    parts = []
    for msg in messages:
        role = msg.get("role", "unknown")
        content = msg.get("content", "")
        if isinstance(content, list):
            text_parts = []
            for block in content:
                if isinstance(block, dict) and block.get("type") in ("text", "input_text"):
                    text_parts.append(str(block.get("text", "")))
            content = " ".join(text_parts)
        else:
            content = str(content)
        parts.append(f"{role.upper()}: {content}")
    return "\n\n".join(parts)


# ---------------------------------------------------------------------------
# Plugin registration
# ---------------------------------------------------------------------------

def register(ctx) -> None:
    """Register plugin hooks, tools, and slash commands."""
    ctx.register_tool(
        name="compress_prompt",
        toolset="prompt-compress",
        schema=COMPRESS_PROMPT_SCHEMA,
        handler=_handle_tool,
        check_fn=check_requirements,
        emoji="\U0001f5dc\ufe0f",
        description="Compress text prompts using token importance scoring to reduce LLM costs.",
    )

    ctx.register_command(
        name="prompt-compress",
        handler=_handle_slash_command,
        description=(
            "Compress text using prompt-compress. "
            "Usage: /prompt-compress <text> [--aggressiveness N] [--model NAME] "
            "[--scorer-mode standard|agent-aware]"
        ),
    )

    ctx.register_hook("on_session_start", _on_session_start)
    ctx.register_hook("pre_llm_call", _pre_llm_call)

    logger.info("prompt-compress plugin registered (CLI backend)")
