# Hermes-Agent Integration Plan

## Goal
Make prompt-compress usable as a compression layer for hermes-agent workflows, reducing token costs on every LLM call.

## Phase 1: Python SDK (Week 1)
- [x] Create `sdk/python/` with pip-installable package
- [x] `PromptCompressor` class wrapping the HTTP API (sync + async)
- [x] Type hints, py.typed marker
- [x] PyPI-ready setup with pyproject.toml (setuptools backend)
- [ ] Integration tests against live API (requires running compress-api server)

## Phase 2: Hermes Skill (Week 1-2)
- [ ] Create hermes skill at `~/.hermes/skills/prompt-compress/`
- [ ] Register as callable tool from hermes agent
- [ ] Pre-compress system prompts before LLM calls
- [ ] Post-compress context windows to extend conversation length

## Phase 3: Agent-Aware Scoring (Week 2)
- [ ] Extend HeuristicScorer with agent-prompt awareness:
  - Protect tool/function definitions (JSON schemas)
  - Protect code blocks (``` fenced regions)
  - Protect markdown headers and structure
  - Higher weight for instruction verbs ("create", "analyze", "fix")
  - Lower weight for conversational filler in agent prompts
- [ ] New scorer mode: `heuristic-agent-v0.1`
- [ ] Add `--scorer-mode` CLI flag

## Phase 4: Hermes Presets (Week 2-3)
- [ ] Pre-configured compression profiles:
  - `hermes-system`: compress system/developer prompts (aggressive=0.3)
  - `hermes-context`: compress accumulated context (aggressive=0.5)
  - `hermes-tools`: compress tool definitions (aggressive=0.2, protect schemas)
  - `hermes-memory`: compress memory/recall entries (aggressive=0.6)
- [ ] Auto-detect prompt type and select preset
- [ ] Add `/v1/compress/preset/<name>` API endpoint

## Phase 5: Deep Integration (Week 3-4)
- [ ] Middleware mode: intercept hermes LLM calls transparently
- [ ] Token budget enforcement: compress to fit within model limits
- [ ] Compression caching: hash-based dedup for repeated prompts
- [ ] Metrics endpoint: track savings per session/agent

## Completed
- Phase 1 (Python SDK): `sdk/python/` created with `PromptCompressor` (sync) and `AsyncPromptCompressor` (async), dataclass models, `py.typed` marker, pyproject.toml with setuptools backend, 8 passing tests.
