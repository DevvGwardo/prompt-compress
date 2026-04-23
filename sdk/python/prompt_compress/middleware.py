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

import hashlib
import json
import logging
import copy
from collections import OrderedDict
from typing import Any, Callable, Optional

from .client import AsyncPromptCompressor, PromptCompressor
from .models import CompressResponse

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Compression cache
# ---------------------------------------------------------------------------

class _CompressionCache:
    """Simple hash-based LRU cache for compression responses."""

    def __init__(self, max_size: int = 128) -> None:
        self._cache: OrderedDict[str, Any] = OrderedDict()
        self.max_size = max_size
        self.hits = 0
        self.misses = 0

    @staticmethod
    def _make_key(text: str, **params: Any) -> str:
        key_data = json.dumps({"text": text, **params}, sort_keys=True, ensure_ascii=True)
        return hashlib.sha256(key_data.encode("utf-8")).hexdigest()

    def get(self, text: str, **params: Any) -> Any | None:
        key = self._make_key(text, **params)
        if key in self._cache:
            self._cache.move_to_end(key)
            self.hits += 1
            return self._cache[key]
        self.misses += 1
        return None

    def set(self, text: str, response: Any, **params: Any) -> None:
        key = self._make_key(text, **params)
        self._cache[key] = response
        self._cache.move_to_end(key)
        if len(self._cache) > self.max_size:
            self._cache.popitem(last=False)


# ---------------------------------------------------------------------------
# Default configuration values
# ---------------------------------------------------------------------------

_DEFAULT_SYSTEM_MIN_CHARS = 150
_DEFAULT_SYSTEM_MIN_SAVINGS = 5
_DEFAULT_CONTEXT_MIN_SAVINGS = 10
_DEFAULT_PROTECTED_TURNS = 2

# Rough heuristic: ~4 characters per token for English text with GPT tokenizers.
_CHARS_PER_TOKEN = 4.0

# Maximum aggressiveness when enforcing a token budget.
_MAX_BUDGET_AGGRESSIVENESS = 0.9

# Step size for increasing aggressiveness during budget enforcement.
_AGGRESSIVENESS_STEP = 0.1


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
        token_budget: Optional[int] = None,
        cache_enabled: bool = False,
        cache_max_size: int = 128,
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
        self.token_budget = token_budget
        self.cache_enabled = cache_enabled
        self.cache_max_size = cache_max_size


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


def _estimate_tokens(messages: list[dict]) -> int:
    """Estimate token count for a message list using a chars-per-token heuristic.

    This is a rough approximation (~4 chars/token) sufficient for budget
    enforcement. For more accurate counts the API itself should be used.
    """
    total_chars = 0
    for msg in messages:
        content = msg.get("content", "")
        if isinstance(content, list):
            text_parts = [
                str(block.get("text", ""))
                for block in content
                if isinstance(block, dict) and block.get("type") in ("text", "input_text")
            ]
            text = " ".join(text_parts)
        else:
            text = str(content)
        total_chars += len(text)
    # Add a small overhead per message for role labels / formatting.
    return int(total_chars / _CHARS_PER_TOKEN) + len(messages) * 2


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
    token_budget:
        Optional maximum token budget. If set, the middleware will
        iteratively increase compression aggressiveness (and, if
        necessary, drop old messages) until the estimated token count
        falls within the budget.
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
        token_budget: Optional[int] = None,
        cache_enabled: bool = False,
        cache_max_size: int = 128,
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
            token_budget=token_budget,
            cache_enabled=cache_enabled,
            cache_max_size=cache_max_size,
        )
        self._cache = _CompressionCache(cache_max_size) if cache_enabled else None
        self.total_input_tokens: int = 0
        self.total_output_tokens: int = 0
        self.total_savings: int = 0
        self.calls_made: int = 0

    @property
    def cache_hits(self) -> int:
        """Number of cache hits since creation."""
        return self._cache.hits if self._cache else 0

    @property
    def cache_misses(self) -> int:
        """Number of cache misses since creation."""
        return self._cache.misses if self._cache else 0

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

        resp = None
        if self._cache is not None:
            resp = self._cache.get(
                system_text, preset=self._cfg.system_preset, target_model=self._cfg.target_model
            )

        if resp is None:
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
            if self._cache is not None:
                self._cache.set(
                    system_text, resp, preset=self._cfg.system_preset, target_model=self._cfg.target_model
                )

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

        resp = None
        if self._cache is not None:
            resp = self._cache.get(
                serialized,
                aggressiveness=0.5,
                target_model=self._cfg.target_model,
                model=self._cfg.scorer_model,
            )

        if resp is None:
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
            if self._cache is not None:
                self._cache.set(
                    serialized,
                    resp,
                    aggressiveness=0.5,
                    target_model=self._cfg.target_model,
                    model=self._cfg.scorer_model,
                )

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

    def _enforce_budget(self, messages: list[dict]) -> list[dict]:
        """Ensure messages fit within ``token_budget`` by increasing aggressiveness.

        If the estimated token count exceeds the budget, the method first
        attempts to re-compress the old context with progressively higher
        aggressiveness. If that still doesn't fit, it drops the oldest
        non-system messages until the budget is satisfied.
        """
        if self._cfg.token_budget is None:
            return messages

        estimated = _estimate_tokens(messages)
        if estimated <= self._cfg.token_budget:
            return messages

        logger.debug(
            "Token budget exceeded: estimated %d > budget %d",
            estimated,
            self._cfg.token_budget,
        )

        working = list(messages)
        protect_count = self._cfg.protected_turns * 2

        # Try increasing aggressiveness on context first.
        if self._cfg.compress_context and len(working) > protect_count:
            aggressiveness = 0.5 + _AGGRESSIVENESS_STEP
            while aggressiveness <= _MAX_BUDGET_AGGRESSIVENESS + 1e-9:
                old_messages = working[: len(working) - protect_count]
                serialized = _serialize_messages(old_messages)
                if not serialized.strip():
                    break
                resp = None
                if self._cache is not None:
                    resp = self._cache.get(
                        serialized,
                        aggressiveness=aggressiveness,
                        target_model=self._cfg.target_model,
                        model=self._cfg.scorer_model,
                    )

                if resp is None:
                    try:
                        resp = self._compressor.compress(
                            serialized,
                            aggressiveness=aggressiveness,
                            target_model=self._cfg.target_model,
                            model=self._cfg.scorer_model,
                        )
                    except Exception as exc:
                        if self._cfg.on_error == "raise":
                            raise
                        if self._cfg.on_error == "warn":
                            logger.warning(
                                "Budget context compression failed at aggressiveness %.1f: %s",
                                aggressiveness,
                                exc,
                            )
                        break
                    if self._cache is not None:
                        self._cache.set(
                            serialized,
                            resp,
                            aggressiveness=aggressiveness,
                            target_model=self._cfg.target_model,
                            model=self._cfg.scorer_model,
                        )

                working = _replace_old_context(working, resp.output, self._cfg.protected_turns)
                estimated = _estimate_tokens(working)
                if estimated <= self._cfg.token_budget:
                    self.total_input_tokens += resp.original_input_tokens
                    self.total_output_tokens += resp.output_tokens
                    self.total_savings += resp.original_input_tokens - resp.output_tokens
                    logger.debug(
                        "Budget enforced via re-compression (%.1f): %d → %d tokens",
                        aggressiveness,
                        resp.original_input_tokens,
                        resp.output_tokens,
                    )
                    return working
                aggressiveness += _AGGRESSIVENESS_STEP

        # Last resort: drop oldest non-system messages.
        while _estimate_tokens(working) > self._cfg.token_budget and len(working) > 1:
            # Find the first non-system message to drop.
            drop_idx = None
            for i, msg in enumerate(working):
                if msg.get("role") != "system":
                    drop_idx = i
                    break
            if drop_idx is None:
                break
            dropped = working.pop(drop_idx)
            logger.debug("Dropped message to meet budget: %s", dropped.get("role", "unknown"))

        return working

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

        # 3. Token budget enforcement
        budgeted = self._enforce_budget(working)
        if budgeted is not working:
            working = budgeted
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
        token_budget: Optional[int] = None,
        cache_enabled: bool = False,
        cache_max_size: int = 128,
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
            token_budget=token_budget,
            cache_enabled=cache_enabled,
            cache_max_size=cache_max_size,
        )
        self._cache = _CompressionCache(cache_max_size) if cache_enabled else None
        self.total_input_tokens: int = 0
        self.total_output_tokens: int = 0
        self.total_savings: int = 0
        self.calls_made: int = 0

    @property
    def cache_hits(self) -> int:
        """Number of cache hits since creation."""
        return self._cache.hits if self._cache else 0

    @property
    def cache_misses(self) -> int:
        """Number of cache misses since creation."""
        return self._cache.misses if self._cache else 0

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

        resp = None
        if self._cache is not None:
            resp = self._cache.get(
                system_text, preset=self._cfg.system_preset, target_model=self._cfg.target_model
            )

        if resp is None:
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
            if self._cache is not None:
                self._cache.set(
                    system_text, resp, preset=self._cfg.system_preset, target_model=self._cfg.target_model
                )

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

        resp = None
        if self._cache is not None:
            resp = self._cache.get(
                serialized,
                aggressiveness=0.5,
                target_model=self._cfg.target_model,
                model=self._cfg.scorer_model,
            )

        if resp is None:
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
            if self._cache is not None:
                self._cache.set(
                    serialized,
                    resp,
                    aggressiveness=0.5,
                    target_model=self._cfg.target_model,
                    model=self._cfg.scorer_model,
                )

        savings = resp.original_input_tokens - resp.output_tokens
        if savings < self._cfg.context_min_savings:
            return None

        self.total_input_tokens += resp.original_input_tokens
        self.total_output_tokens += resp.output_tokens
        self.total_savings += savings
        return _replace_old_context(messages, resp.output, self._cfg.protected_turns)

    async def _enforce_budget(self, messages: list[dict]) -> list[dict]:
        """Async version of budget enforcement."""
        if self._cfg.token_budget is None:
            return messages

        estimated = _estimate_tokens(messages)
        if estimated <= self._cfg.token_budget:
            return messages

        logger.debug(
            "Token budget exceeded: estimated %d > budget %d",
            estimated,
            self._cfg.token_budget,
        )

        working = list(messages)
        protect_count = self._cfg.protected_turns * 2

        if self._cfg.compress_context and len(working) > protect_count:
            aggressiveness = 0.5 + _AGGRESSIVENESS_STEP
            while aggressiveness <= _MAX_BUDGET_AGGRESSIVENESS + 1e-9:
                old_messages = working[: len(working) - protect_count]
                serialized = _serialize_messages(old_messages)
                if not serialized.strip():
                    break
                resp = None
                if self._cache is not None:
                    resp = self._cache.get(
                        serialized,
                        aggressiveness=aggressiveness,
                        target_model=self._cfg.target_model,
                        model=self._cfg.scorer_model,
                    )

                if resp is None:
                    try:
                        resp = await self._compressor.compress(
                            serialized,
                            aggressiveness=aggressiveness,
                            target_model=self._cfg.target_model,
                            model=self._cfg.scorer_model,
                        )
                    except Exception as exc:
                        if self._cfg.on_error == "raise":
                            raise
                        if self._cfg.on_error == "warn":
                            logger.warning(
                                "Budget context compression failed at aggressiveness %.1f: %s",
                                aggressiveness,
                                exc,
                            )
                        break
                    if self._cache is not None:
                        self._cache.set(
                            serialized,
                            resp,
                            aggressiveness=aggressiveness,
                            target_model=self._cfg.target_model,
                            model=self._cfg.scorer_model,
                        )

                working = _replace_old_context(working, resp.output, self._cfg.protected_turns)
                estimated = _estimate_tokens(working)
                if estimated <= self._cfg.token_budget:
                    self.total_input_tokens += resp.original_input_tokens
                    self.total_output_tokens += resp.output_tokens
                    self.total_savings += resp.original_input_tokens - resp.output_tokens
                    logger.debug(
                        "Budget enforced via re-compression (%.1f): %d → %d tokens",
                        aggressiveness,
                        resp.original_input_tokens,
                        resp.output_tokens,
                    )
                    return working
                aggressiveness += _AGGRESSIVENESS_STEP

        while _estimate_tokens(working) > self._cfg.token_budget and len(working) > 1:
            drop_idx = None
            for i, msg in enumerate(working):
                if msg.get("role") != "system":
                    drop_idx = i
                    break
            if drop_idx is None:
                break
            dropped = working.pop(drop_idx)
            logger.debug("Dropped message to meet budget: %s", dropped.get("role", "unknown"))

        return working

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

        # 3. Token budget enforcement
        budgeted = await self._enforce_budget(working)
        if budgeted is not working:
            working = budgeted
            modified = True

        if modified:
            kwargs = {**kwargs, "messages": working}
            self.calls_made += 1

        return await self._client(*args, **kwargs)
