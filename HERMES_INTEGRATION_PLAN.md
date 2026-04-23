# Hermes-Agent Integration Plan

## Goal
Make prompt-compress usable as a compression layer for hermes-agent workflows, reducing token costs on every LLM call.

## Phase 1: Python SDK (Week 1)
- [x] Create `sdk/python/` with pip-installable package
- [x] `PromptCompressor` class wrapping the HTTP API (sync + async)
- [x] Type hints, py.typed marker
- [x] PyPI-ready setup with pyproject.toml (setuptools backend)
- [x] Integration tests against live API (requires running compress-api server)
  - Verified: 14 base integration tests pass (sync + async, aggressiveness, content quality)
  - Added 6 preset endpoint integration tests (system, memory, tools, context, invalid preset 400, async)
  - Total Python SDK tests: 28 passing

## Phase 2: Hermes Skill (Week 1-2)
- [x] Create hermes skill at `~/.hermes/skills/prompt-compress/` (source: `hermes-skill/SKILL.md`, installed to `software-development/prompt-compress`)
- [x] Register as callable tool from hermes agent (Python plugin installed to `~/.hermes/plugins/prompt-compress/`, SDK installed in venv)
- [x] Pre-compress system prompts before LLM calls
  - Implemented in `hermes_plugin/__init__.py` `_pre_llm_call` hook: detects `role="system"` messages, compresses using `system` preset (aggressiveness 0.3) via `compress_preset()` if >150 chars and saves ≥5 tokens
  - Returns compressed system prompt in dict under `system_prompt` key
  - Gracefully handles errors (logs warning, returns None)
- [x] Post-compress context windows to extend conversation length
  - Implemented in same `_pre_llm_call` hook: preserves last 2 turns, compresses older history using `heuristic-agent-v0.1` model with adaptive aggressiveness (0.4–0.6)
  - 25 plugin unit tests added covering system prompt extraction, context serialization, pre_llm_call behavior, tool handler, slash command, and error handling
  - All tests pass (`PYTHONPATH=. pytest hermes_plugin/tests/test_plugin.py -v` → 25 passed)

## Phase 3: Agent-Aware Scoring (Week 2)
- [x] Extend HeuristicScorer with agent-prompt awareness (HeuristicMode enum, with Standard/AgentAware, agent-aware scoring logic: boosted instruction verbs, extra filler stop-words, calibrated importance weights)
  - Implemented: `HeuristicMode` enum, `HeuristicScorer::with_mode()`, `word_importance()` with mode-aware scoring, protected token handling for `<ttc_safe>` regions preserved
  - New stop-word lists: Standard vs AgentAware (extra conversational filler)
  - Instruction verbs ("create", "analyze", "fix", etc.) score 0.95 in agent mode
  - Added agent-aware unit tests
  - Updated all construction sites, fixed missing `scorer_mode` field across codebase
  - `cargo check --all && cargo test --all` pass (139 tests total)
- [x] New scorer model ID: `heuristic-agent-v0.1` (expose in API routing)
  - Already implemented in `routes.rs` — `heuristic-agent-v0.1` maps to `HeuristicMode::AgentAware`
- [x] Add `--scorer-mode` CLI flag to compress-cli
  - Already implemented in `compress-cli/src/main.rs` — `--scorer-mode` with `standard` (default) and `agent-aware` variants


## Phase 4: Hermes Presets (Week 2-3)
- [x] Pre-configured compression profiles:
  - `hermes-system`: compress system/developer prompts (aggressive=0.3)
  - `hermes-context`: compress accumulated context (aggressive=0.5)
  - `hermes-tools`: compress tool definitions (aggressive=0.2, protect schemas)
  - `hermes-memory`: compress memory/recall entries (aggressive=0.6)
- [x] Auto-detect prompt type and select preset
  - Implemented: `POST /v1/compress/detect` endpoint with `detect_preset()` heuristics
  - Detects `tools` by JSON/schema density (braces, quotes, keywords like "type"/"properties"/"function")
  - Detects `system` by instruction markers ("you are", "your task", "must", "should", etc.) — 2+ hits
  - Detects `memory` by recall markers ("earlier", "we discussed", "you said", etc.) — 2+ hits
  - Falls back to `context` for generic text
  - Returns `CompressDetectResponse` with `detected_preset` field
  - 5 Rust unit tests added (tools, system, memory, context, empty input)
  - Python SDK: `PromptCompressor.compress_detect()` and `AsyncPromptCompressor.compress_detect()`
  - Python SDK unit tests: `CompressDetectResponse` model + `_parse_detect_response` helper
  - Python integration tests: 4 sync + 1 async detect test cases added
- [x] Add `/v1/compress/preset/<name>` API endpoint
  - Implemented: `POST /v1/compress/preset/{name}` with presets `system` (0.3), `context` (0.5), `tools` (0.2), `memory` (0.6)
  - Uses `HeuristicMode::AgentAware` by default for agent-optimized scoring
  - Accepts optional `target_model` in request body
  - Returns `CompressPresetResponse` with `preset` field included
  - 4 unit tests added (system preset, memory preset, invalid preset 400, target_model passthrough)
  - `cargo check --all && cargo test --all` pass (153 tests total)

## Phase 5: Deep Integration (Week 3-4)
- [x] Middleware mode: intercept hermes LLM calls transparently
  - Implemented `CompressMiddleware` (sync) and `AsyncCompressMiddleware` (async) in `sdk/python/prompt_compress/middleware.py`
  - Wraps any callable that accepts `messages` kwarg (e.g. OpenAI/Anthropic client methods)
  - Transparently compresses system prompts (configurable min chars, min savings, preset)
  - Transparently compresses old conversation context (protects N recent user+assistant turn pairs)
  - Tracks cumulative metrics: `total_input_tokens`, `total_output_tokens`, `total_savings`, `calls_made`, `compression_ratio`
  - Error handling modes: `warn` (log + continue), `raise`, `ignore`
  - Supports multimodal content blocks (extracts text parts)
  - 26 Python SDK unit tests added (helpers + sync + async middleware), all passing
- [x] Token budget enforcement: compress to fit within model limits
  - Added `token_budget` parameter to `CompressMiddleware` and `AsyncCompressMiddleware`
  - `_estimate_tokens()` helper uses ~4 chars/token heuristic + message overhead
  - When budget is exceeded, iteratively re-compresses context with increasing aggressiveness (0.6 → 0.9)
  - Falls back to dropping oldest non-system messages if re-compression insufficient
  - Tracks savings in middleware metrics
  - 9 new unit tests: `_estimate_tokens` (4), sync budget enforcement (4), async budget enforcement (1)
  - All 35 middleware tests pass; 158 Rust tests pass
- [x] Compression caching: hash-based dedup for repeated prompts
  - Implemented `_CompressionCache` class in `middleware.py`: SHA-256 hash keys, LRU eviction, configurable max size
  - Integrated into both `CompressMiddleware` and `AsyncCompressMiddleware` for system prompts, context, and budget enforcement
  - Added `cache_enabled` (default False) and `cache_max_size` (default 128) parameters
  - Exposed `cache_hits` and `cache_misses` properties
  - Added 6 unit tests: disabled by default, sync system/context, async system/context, eviction
  - All 41 middleware tests pass; 158 Rust tests pass
- [x] Metrics endpoint: track savings per session/agent
  - Rust: `GET /v1/metrics` endpoint already returns `MetricsResponse` with per-session entries and overall totals
  - Python SDK: `MetricsEntry` and `MetricsResponse` dataclass models added and exported in `__init__.py`
  - `CompressRequest` updated with `session_id` and `agent` optional fields; payload builder omits None values
  - Added Python SDK unit tests: `MetricsEntry` model, `MetricsResponse` model, payload with/without session_id+agent (15 total Python tests passing)
  - Added Rust unit tests for metrics endpoint: empty initially, tracks single compression, session_id+agent, filters by session_id, filters by agent, aggregates multiple calls (6 tests, all passing)
  - Total Rust tests: 164 passing

## Phase 6: Production Hardening (Ongoing)
- [x] Health check support in Python SDK
  - Added `health_check()` to `PromptCompressor` and `AsyncPromptCompressor` — probes `/health` endpoint and returns `True`/`False`
  - 6 unit tests: sync ok/fail/exception + async ok/fail/exception (all passing)
  - Useful for hermes plugin to verify server availability before compression attempts
- [x] Update main README.md with Python SDK and Hermes integration docs
  - Added "Python SDK" section with install instructions, basic/async usage, and middleware mode examples
  - Added "Hermes Agent Integration" section with skill overview, features, and enablement config
  - Updated repository layout diagram to include `sdk/python/`, `hermes-skill/`, and `hermes_plugin/`

## Completed
- Phase 1 (Python SDK): `sdk/python/` created with `PromptCompressor` (sync) and `AsyncPromptCompressor` (async), dataclass models, `py.typed` marker, pyproject.toml with setuptools backend, 8 passing unit tests.
- Phase 2 (Hermes Skill): SKILL.md created and installed to `~/.hermes/skills/software-development/prompt-compress/`. Covers SDK usage, CLI usage, presets, endpoints, error handling, and tips. Includes `install.sh`.
- Phase 3 (Agent-Aware Scoring): Rust infrastructure complete. `HeuristicMode::AgentAware` implemented with instruction-verb boosting and expanded stop-word demotion for conversational filler. All unit tests (18 scorer + 17 compressor + others) pass. CLI flag pending.
