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
- [x] Create hermes skill at `~/.hermes/skills/prompt-compress/` (source: `hermes-skill/SKILL.md`, installed to `software-development/prompt-compress`)
- [x] Register as callable tool from hermes agent (Python plugin installed to `~/.hermes/plugins/prompt-compress/`, SDK installed in venv)
- [ ] Pre-compress system prompts before LLM calls
- [ ] Post-compress context windows to extend conversation length

## Phase 3: Agent-Aware Scoring (Week 2)
- [x] Extend HeuristicScorer with agent-prompt awareness (HeuristicMode enum, with Standard/AgentAware, agent-aware scoring logic: boosted instruction verbs, extra filler stop-words, calibrated importance weights)
  - Implemented: `HeuristicMode` enum, `HeuristicScorer::with_mode()`, `word_importance()` with mode-aware scoring, protected token handling for `<ttc_safe>` regions preserved
  - New stop-word lists: Standard vs AgentAware (extra conversational filler)
  - Instruction verbs ("create", "analyze", "fix", etc.) score 0.95 in agent mode
  - Added agent-aware unit tests
  - Updated all construction sites, fixed missing `scorer_mode` field across codebase
  - `cargo check --all && cargo test --all` pass (139 tests total)
- [ ] New scorer model ID: `heuristic-agent-v0.1` (expose in API routing)
- [ ] Add `--scorer-mode` CLI flag to compress-cli


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
- Phase 1 (Python SDK): `sdk/python/` created with `PromptCompressor` (sync) and `AsyncPromptCompressor` (async), dataclass models, `py.typed` marker, pyproject.toml with setuptools backend, 8 passing unit tests.
- Phase 2 (Hermes Skill): SKILL.md created and installed to `~/.hermes/skills/software-development/prompt-compress/`. Covers SDK usage, CLI usage, presets, endpoints, error handling, and tips. Includes `install.sh`.
- Phase 3 (Agent-Aware Scoring): Rust infrastructure complete. `HeuristicMode::AgentAware` implemented with instruction-verb boosting and expanded stop-word demotion for conversational filler. All unit tests (18 scorer + 17 compressor + others) pass. CLI flag pending.
