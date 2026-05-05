---
name: prompt-compress
description: Compress prompts via the compress CLI binary — tool, slash command, and auto-compression hooks for Hermes Agent.
category: software-development
version: 1.1.0
author: prompt-compress
metadata:
  hermes:
    tags: [compression, tokens, cost-optimization, llm]
    homepage: https://github.com/DevvGwardo/prompt-compress
prerequisites:
  commands: [compress]
  environment_variables:
    - PROMPT_COMPRESS_BIN (optional, path to compress binary)
---

# Prompt Compress

Compress prompts and context windows to reduce LLM token costs. Uses the `compress` CLI binary (Rust, heuristic scoring — no ONNX model needed) to score token importance and remove low-value tokens while preserving meaning.

## Integration with Hermes Agent

The `prompt-compress` plugin is installed at `~/.hermes/plugins/prompt-compress/` and enabled in config.yaml. It provides:

- `/prompt-compress` slash command — compress text inline
- `compress_prompt` tool — LLM-callable tool for on-demand compression
- `pre_llm_call` hook — auto-compresses system prompts & old context on every turn

### How It Works

The plugin calls the `compress` CLI binary directly via subprocess. No Python SDK, no HTTP API server, no external dependencies. The binary uses heuristic scoring by default (fast, no model download needed).

Binary location: `~/prompt-compress/target/release/compress`

### Plugin Status

Check if the plugin is loaded:

```
hermes plugins list
```

Or in Hermes TUI, the tool should show up in the available tools list.

## When to Use

- System prompts are getting long and repetitive
- Context windows are approaching model limits
- You want to reduce token costs on high-volume LLM calls
- Compressing accumulated conversation history
- Shrinking tool/function definition payloads

## Using the Tool

The `compress_prompt` tool accepts:

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `text` | string | required | Text to compress |
| `aggressiveness` | number | 0.5 | 0.0 (minimal) to 1.0 (max) |
| `target_model` | string | gpt-4 | Model for token counting |
| `preset` | string | — | system(0.3), context(0.5), tools(0.2), memory(0.6) |
| `scorer_mode` | string | agent-aware | standard or agent-aware |

## Using the Slash Command

```
/prompt-compress <text> [--aggressiveness N] [--model NAME] [--scorer-mode standard|agent-aware]
```

Examples:

```
/prompt-compress This is a very long prompt that I want to compress to save tokens
/prompt-compress Some text here --aggressiveness 0.7 --model claude-3-opus
/prompt-compress Tools definitions here --scorer-mode standard --aggressiveness 0.2
```

## Compression Presets

| Preset | Aggressiveness | Use Case |
|--------|---------------|----------|
| `system` | 0.3 | System/developer prompts — keep core instructions |
| `context` | 0.5 | Accumulated context — balance detail vs size |
| `tools` | 0.2 | Tool definitions — preserve schemas and structure |
| `memory` | 0.6 | Memory/recall entries — aggressive, key facts only |

## Scorer Modes

- `agent-aware` (default) — Optimized for agent instructions and tool calls. Boosts instruction verbs, demotes conversational filler.
- `standard` — General-purpose text compression. Use for non-agent content.

## Auto-Compression Hooks

The `pre_llm_call` hook runs before every LLM call and:

1. **System prompt compression** — compresses role=system messages using the `system` preset when >150 chars
2. **Context window compression** — preserves last 2 turns, compresses everything before that when there are 4+ messages

Both hooks stop running if they save fewer than 5-10 tokens (diminishing returns).

## CLI Usage (direct)

```bash
# Compress from stdin
echo "Your long text here..." | compress --format json --stats

# Compress with options
compress -i "Your text" -a 0.5 -m gpt-4 --scorer-mode agent-aware --format json

# Show human-readable stats
compress -i "Your text" -a 0.5 --stats
```

## Compression Response

```json
{
  "output": "compressed text...",
  "output_tokens": 18,
  "original_input_tokens": 27,
  "compression_ratio": 0.67
}
```

## Tips

- Start with lower aggressiveness (0.2-0.3) and increase if savings are insufficient
- Always check `compression_ratio` — if > 0.9, compression isn't helping much
- The `target_model` parameter affects token counting (use the model you'll send to)
- Compressed output preserves semantic meaning but may lose exact wording
- Do NOT compress code blocks, JSON schemas, or structured data with high aggressiveness
