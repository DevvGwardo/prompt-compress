"""Middleware that transparently compresses prompts before LLM calls.

Usage::

    from prompt_compress import PromptCompressor, CompressMiddleware

    compressor = PromptCompressor()
    openai_client = openai.OpenAI()

    # Wrap the LLM call
    llm = CompressMiddleware(
        client=openai_client.chat.completions.create,
        compressor=compressor,
    )

    # Pass messages as normal — compression happens transparently
    response = llm(
        model="gpt-4",
        messages=[
            {"role": "system", "content": "You are a helpful assistant..."},
            {"role": "user", "content": "Hello!"},
        ],
    )
"""

from __future__ import annotations

import copy
import functools
import logging
from typing import Any, Callable

from .client import AsyncPromptCompressor, PromptCompressor
from .models import CompressResponse

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Default configuration values
# ---------------------------------------------------------------------------

_DEFAULT_SYSTEM_MIN_CHARS = 150
_DEFAULT_SYSTEM_MIN_SAVINGS = 5
_DEFAULT_CONTEXT_MIN_SAVINGS = 10
_DEFAULT_PROTECTED_TURNS = 2


class _MiddlewareConfig:
    """Shared configuration for sync and async middleware."""

    def __init__(
        self,
        *,
        compress_system: bool = True,
        compress_context: bool = True,
        system_preset: str = "system",
        context_preset: str = "context",
        system_min_chars: int = _DEFAULT_SYSTEM_MIN_CHARS,
        system_min_savings: int = _DEFAULT_SYSTEM_MIN_SAVINGS,
        context_min_savings: int = _DEFAULT_CONTEXT_MIN_SAVINGS,
        protected_turns: int = _DEFAULT_PROTECTED_TURNS,
        target_model: str = "gpt-4",
        scorer_model: str = "heuristic-agent-v0.1",
        on_error: str = "warn",
    ) -> None:
        self.compress_system = compress_system
        self.compress_context = compress_context
        self.system_preset = system_preset
        self.context_preset = context_preset
        self.system_min_chars = system_min_chars
        self.system_min_savings = system_min_savings
        self.context_min_savings = context_min_savings
        self.protected_turns = protected_turns
        self.target_model = target_model
        self.scorer_model = scorer_model
        self.on_error = on_error  # "warn", "raise", "ignore"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _extract_system_text(messages: list[dict]) -> str:
    """Concatenate all system message contents."""
    parts = []
    for msg in messages:
        if msg.get("role") == "system":
            content = msg.get("content", "")
            if isinstance(content, list):
                text_parts = [
                    str(block.get("text", ""))
                    for block in content
                    if isinstance(block, dict) and block.get("type") in ("text", "input_text")
                ]
                content = " ".join(text_parts)
            else:
                content = str(content)
            if content.strip():
                parts.append(content.strip())
    return "\n\n".join(parts)


def _replace_system_messages(messages: list[dict], replacement: str) -> list[dict]:
    """Return a new message list with all system messages replaced by a single one."""
    result = []
    system_replaced = False
    for msg in messages:
        if msg.get("role") == "system":
            if not system_replaced:
                result.append({"role": "system", "content": replacement})
                system_replaced = True
            # Drop additional system messages
        else:
            result.append(copy.deepcopy(msg))
    return result


def _serialize_messages(messages: list[dict]) -> str:
    """Serialize messages into plain text for compression."""
    parts = []
    for msg in messages:
        role = msg.get("role", "unknown")
        content = msg.get("content", "")
        if isinstance(content, list):
            text_parts = [
                str(block.get("text", ""))
                for block in content
                if isinstance(block, dict) and block.get("type") in ("text", "input_text")
            ]
            content = " ".join(text_parts)
        else:
            content = str(content)
        parts.append(f"{role.upper()}: {content}")
    return "\n\n".join(parts)


def _replace_old_context(
    messages: list[dict],
    replacement: str,
    protected_turns: int,
) -> list[dict]:
    """Replace all but the last *protected_turns* (user+assistant pairs) with a summary."""
    protect_count = protected_turns * 2
    if len(messages) <= protect_count:
        return copy.deepcopy(messages)

    preserved = copy.deepcopy(messages[-protect_count:])
    summary_msg = {
        "role": "system",
        "content": f"[Compressed context from earlier turns]:\n{replacement}",
    }
    return [summary_msg] + preserved


# ---------------------------------------------------------------------------
# Sync middleware
# ---------------------------------------------------------------------------

class CompressMiddleware:
    """Transparent compression middleware for synchronous LLM calls.

    Wraps any callable that accepts a ``messages`` keyword argument
    (e.g. ``openai.OpenAI().chat.completions.create``). Before each
    call it compresses system prompts and/or stale conversation context,
    then forwards the modified ``messages`` to the underlying client.

    Parameters
    ----------
    client:
        The callable to wrap. Must accept ``messages`` as a keyword arg.
    compressor:
        A ``PromptCompressor`` instance (or any object with
        ``compress_preset`` and ``compress`` methods).
    compress_system:
        Whether to compress ``role="system"`` messages.
    compress_context:
        Whether to compress old conversation turns.
    system_preset:
        Preset name for system-prompt compression.
    context_preset:
        Preset name for context compression.
    system_min_chars:
        Only compress system prompts longer than this.
    system_min_savings:
        Only keep compressed system prompt if it saves at least this many tokens.
    context_min_savings:
        Only keep compressed context if it saves at least this many tokens.
    protected_turns:
        Number of recent user+assistant turn pairs to preserve untouched.
    target_model:
        Target model string forwarded to the API for token counting.
    scorer_model:
        Scorer model ID forwarded to the API for context compression.
    on_error:
        How to handle compression failures: ``"warn"`` (log and continue
        with original messages), ``"raise"``, or ``"ignore"``.
    """

    def __init__(
        self,
        client: Callable,
        compressor: PromptCompressor,
        *,
        compress_system: bool = True,
        compress_context: bool = True,
        system_preset: str = "system",
        context_preset: str = "context",
        system_min_chars: int = _DEFAULT_SYSTEM_MIN_CHARS,
        system_min_savings: int = _DEFAULT_SYSTEM_MIN_SAVINGS,
        context_min_savings: int = _DEFAULT_CONTEXT_MIN_SAVINGS,
        protected_turns: int = _DEFAULT_PROTECTED_TURNS,
        target_model: str = "gpt-4",
        scorer_model: str = "heuristic-agent-v0.1",
        on_error: str = "warn",
    ) -> None:
        self._client = client
        self._compressor = compressor
        self._cfg = _MiddlewareConfig(
            compress_system=compress_system,
            compress_context=compress_context,
            system_preset=system_preset,
            context_preset=context_preset,
            system_min_chars=system_min_chars,
            system_min_savings=system_min_savings,
            context_min_savings=context_min_savings,
            protected_turns=protected_turns,
            target_model=target_model,
            scorer_model=scorer_model,
            on_error=on_error,
        )
        self.total_input_tokens: int = 0
        self.total_output_tokens: int = 0
        self.total_savings: int = 0
        self.calls_made: int = 0

    @property
    def compression_ratio(self) -> float:
        """Overall compression ratio across all intercepted calls."""
        if self.total_input_tokens == 0:
            return 0.0
        return self.total_savings / self.total_input_tokens

    def _maybe_compress_system(self, messages: list[dict]) -> list[dict] | None:
        """Compress system prompts if enabled and beneficial.

        Returns the modified message list or *None* if no compression occurred.
        """
        if not self._cfg.compress_system:
            return None

        system_text = _extract_system_text(messages)
        if not system_text or len(system_text) < self._cfg.system_min_chars:
            return None

        try:
            resp = self._compressor.compress_preset(
                system_text,
                self._cfg.system_preset,
                target_model=self._cfg.target_model,
            )
        except Exception as exc:
            if self._cfg.on_error == "raise":
                raise
            if self._cfg.on_error == "warn":
                logger.warning("System prompt compression failed: %s", exc)
            return None

        savings = resp.original_input_tokens - resp.output_tokens
        if savings < self._cfg.system_min_savings:
            return None

        self.total_input_tokens += resp.original_input_tokens
        self.total_output_tokens += resp.output_tokens
        self.total_savings += savings
        logger.debug(
            "Compressed system prompt: %d → %d tokens (saved %d)",
            resp.original_input_tokens,
            resp.output_tokens,
            savings,
        )
        return _replace_system_messages(messages, resp.output)

    def _maybe_compress_context(self, messages: list[dict]) -> list[dict] | None:
        """Compress old conversation context if enabled and beneficial.

        Returns the modified message list or *None* if no compression occurred.
        """
        if not self._cfg.compress_context:
            return None

        protect_count = self._cfg.protected_turns * 2
        if len(messages) <= protect_count:
            return None

        old_messages = messages[: len(messages) - protect_count]
        serialized = _serialize_messages(old_messages)
        if not serialized.strip():
            return None

        try:
            resp = self._compressor.compress(
                serialized,
                aggressiveness=0.5,
                target_model=self._cfg.target_model,
                model=self._cfg.scorer_model,
            )
        except Exception as exc:
            if self._cfg.on_error == "raise":
                raise
            if self._cfg.on_error == "warn":
                logger.warning("Context compression failed: %s", exc)
            return None

        savings = resp.original_input_tokens - resp.output_tokens
        if savings < self._cfg.context_min_savings:
            return None

        self.total_input_tokens += resp.original_input_tokens
        self.total_output_tokens += resp.output_tokens
        self.total_savings += savings
        logger.debug(
            "Compressed context: %d → %d tokens (saved %d)",
            resp.original_input_tokens,
            resp.output_tokens,
            savings,
        )
        return _replace_old_context(messages, resp.output, self._cfg.protected_turns)

    def __call__(self, *args, **kwargs) -> Any:
        """Intercept the call, compress messages, and forward to the client."""
        messages = kwargs.get("messages")
        if not messages:
            return self._client(*args, **kwargs)

        modified = False
        working = list(messages)

        # 1. System prompt compression
        result = self._maybe_compress_system(working)
        if result is not None:
            working = result
            modified = True

        # 2. Context compression (operates on original or system-compressed list)
        result = self._maybe_compress_context(working)
        if result is not None:
            working = result
            modified = True

        if modified:
            kwargs = {**kwargs, "messages": working}
            self.calls_made += 1

        return self._client(*args, **kwargs)


# ---------------------------------------------------------------------------
# Async middleware
# ---------------------------------------------------------------------------

class AsyncCompressMiddleware:
    """Transparent compression middleware for asynchronous LLM calls.

    Same behaviour as ``CompressMiddleware`` but wraps an async callable.
    """

    def __init__(
        self,
        client: Callable,
        compressor: AsyncPromptCompressor,
        *,
        compress_system: bool = True,
        compress_context: bool = True,
        system_preset: str = "system",
        context_preset: str = "context",
        system_min_chars: int = _DEFAULT_SYSTEM_MIN_CHARS,
        system_min_savings: int = _DEFAULT_SYSTEM_MIN_SAVINGS,
        context_min_savings: int = _DEFAULT_CONTEXT_MIN_SAVINGS,
        protected_turns: int = _DEFAULT_PROTECTED_TURNS,
        target_model: str = "gpt-4",
        scorer_model: str = "heuristic-agent-v0.1",
        on_error: str = "warn",
    ) -> None:
        self._client = client
        self._compressor = compressor
        self._cfg = _MiddlewareConfig(
            compress_system=compress_system,
            compress_context=compress_context,
            system_preset=system_preset,
            context_preset=context_preset,
            system_min_chars=system_min_chars,
            system_min_savings=system_min_savings,
            context_min_savings=context_min_savings,
            protected_turns=protected_turns,
            target_model=target_model,
            scorer_model=scorer_model,
            on_error=on_error,
        )
        self.total_input_tokens: int = 0
        self.total_output_tokens: int = 0
        self.total_savings: int = 0
        self.calls_made: int = 0

    @property
    def compression_ratio(self) -> float:
        """Overall compression ratio across all intercepted calls."""
        if self.total_input_tokens == 0:
            return 0.0
        return self.total_savings / self.total_input_tokens

    async def _maybe_compress_system(self, messages: list[dict]) -> list[dict] | None:
        if not self._cfg.compress_system:
            return None

        system_text = _extract_system_text(messages)
        if not system_text or len(system_text) < self._cfg.system_min_chars:
            return None

        try:
            resp = await self._compressor.compress_preset(
                system_text,
                self._cfg.system_preset,
                target_model=self._cfg.target_model,
            )
        except Exception as exc:
            if self._cfg.on_error == "raise":
                raise
            if self._cfg.on_error == "warn":
                logger.warning("System prompt compression failed: %s", exc)
            return None

        savings = resp.original_input_tokens - resp.output_tokens
        if savings < self._cfg.system_min_savings:
            return None

        self.total_input_tokens += resp.original_input_tokens
        self.total_output_tokens += resp.output_tokens
        self.total_savings += savings
        return _replace_system_messages(messages, resp.output)

    async def _maybe_compress_context(self, messages: list[dict]) -> list[dict] | None:
        if not self._cfg.compress_context:
            return None

        protect_count = self._cfg.protected_turns * 2
        if len(messages) <= protect_count:
            return None

        old_messages = messages[: len(messages) - protect_count]
        serialized = _serialize_messages(old_messages)
        if not serialized.strip():
            return None

        try:
            resp = await self._compressor.compress(
                serialized,
                aggressiveness=0.5,
                target_model=self._cfg.target_model,
                model=self._cfg.scorer_model,
            )
        except Exception as exc:
            if self._cfg.on_error == "raise":
                raise
            if self._cfg.on_error == "warn":
                logger.warning("Context compression failed: %s", exc)
            return None

        savings = resp.original_input_tokens - resp.output_tokens
        if savings < self._cfg.context_min_savings:
            return None

        self.total_input_tokens += resp.original_input_tokens
        self.total_output_tokens += resp.output_tokens
        self.total_savings += savings
        return _replace_old_context(messages, resp.output, self._cfg.protected_turns)

    async def __call__(self, *args, **kwargs) -> Any:
        messages = kwargs.get("messages")
        if not messages:
            return await self._client(*args, **kwargs)

        modified = False
        working = list(messages)

        result = await self._maybe_compress_system(working)
        if result is not None:
            working = result
            modified = True

        result = await self._maybe_compress_context(working)
        if result is not None:
            working = result
            modified = True

        if modified:
            kwargs = {**kwargs, "messages": working}
            self.calls_made += 1

        return await self._client(*args, **kwargs)
