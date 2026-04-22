---
name: prompt-compress
description: Compress prompts and context windows to reduce LLM token costs. Uses the prompt-compress toolkit to score token importance and remove low-value tokens before sending to LLMs.
category: software-development
version: 1.0.0
author: prompt-compress
metadata:
  hermes:
    tags: [compression, tokens, cost-optimization, llm]
    homepage: https://github.com/DevvGwardo/prompt-compress
prerequisites:
  commands: [python3]
  pip_packages: [prompt-compress]
  environment_variables:
    - PROMPT_COMPRESS_BASE_URL (optional, default: http://localhost:3000)
---

# Prompt Compress

Compress prompts and context windows to reduce LLM token costs. The prompt-compress toolkit scores token importance and removes low-value tokens while preserving meaning.

## When to Use

- System prompts are getting long and repetitive
- Context windows are approaching model limits
- You want to reduce token costs on high-volume LLM calls
- Compressing accumulated conversation history
- Shrinking tool/function definition payloads

## Quick Start

### Python SDK (Recommended)

```python
from prompt_compress import PromptCompressor

# Initialize (requires compress-api running on localhost:3000)
client = PromptCompressor(base_url="http://localhost:3000")

# Compress a prompt
result = client.compress(
    "Your long prompt text here...",
    aggressiveness=0.5,  # 0.0 = no compression, 1.0 = aggressive
    target_model="gpt-4",
)
print(f"Saved {result.original_input_tokens - result.output_tokens} tokens")
print(f"Compressed: {result.output}")
```

### Async Usage

```python
from prompt_compress import AsyncPromptCompressor

async with AsyncPromptCompressor(base_url="http://localhost:3000") as client:
    result = await client.compress("Your text...", aggressiveness=0.3)
```

### CLI Usage

```bash
# Direct compression via CLI
prompt-compress compress --text "Your long prompt here" --aggressiveness 0.5

# Or via the API
curl -X POST http://localhost:3000/v1/compress \
  -H "Content-Type: application/json" \
  -d '{
    "model": "scorer-v0.1",
    "input": "Your long prompt here...",
    "compression_settings": {
      "aggressiveness": 0.5,
      "target_model": "gpt-4"
    }
  }'
```

## Compression Presets for Hermes

Use these aggressiveness levels based on what you are compressing:

| Preset | Aggressiveness | Use Case |
|--------|---------------|----------|
| `system` | 0.3 | System/developer prompts — keep core instructions |
| `context` | 0.5 | Accumulated context — balance detail vs size |
| `tools` | 0.2 | Tool definitions — preserve schemas and structure |
| `memory` | 0.6 | Memory/recall entries — aggressive, key facts only |

### Choosing a Preset

```python
from prompt_compress import PromptCompressor

client = PromptCompressor()

# Compress a system prompt (conservative)
result = client.compress(system_prompt, aggressiveness=0.3, target_model="gpt-4")

# Compress accumulated context (balanced)
result = client.compress(context_window, aggressiveness=0.5, target_model="gpt-4")

# Compress tool definitions (very conservative — schemas must survive)
result = client.compress(tool_defs, aggressiveness=0.2, target_model="gpt-4")

# Compress memory entries (aggressive — just the facts)
result = client.compress(memory_text, aggressiveness=0.6, target_model="gpt-4")
```

## Compression Response

```python
@dataclass
class CompressResponse:
    output: str              # The compressed text
    output_tokens: int       # Token count after compression
    original_input_tokens: int  # Token count before compression
    compression_ratio: float # Ratio of output/input (lower = more compressed)
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/compress` | Compress text directly |
| POST | `/v1/chat/completions` | Proxy with auto-compression (OpenAI format) |
| POST | `/v1/messages` | Proxy with auto-compression (Anthropic format) |

## Supported Models

- `scorer-v0.1` — ML-based token importance scorer (default, higher quality)
- `heuristic-v0.1` — Rule-based scorer (faster, no ML model needed)

## Error Handling

```python
from prompt_compress import PromptCompressor
import httpx

client = PromptCompressor()

try:
    result = client.compress("Some text")
except httpx.HTTPStatusError as e:
    # API returned 4xx/5xx
    print(f"API error: {e.response.status_code} - {e.response.text}")
except httpx.ConnectError:
    # compress-api not running
    print("Cannot connect to compress-api. Is it running on localhost:3000?")
```

## Tips

- Start with lower aggressiveness (0.2-0.3) and increase if savings are insufficient
- Always check `compression_ratio` — if > 0.9, compression isn't helping much
- The `target_model` parameter affects token counting (use the model you'll send to)
- Compressed output preserves semantic meaning but may lose exact wording
- Do NOT compress code blocks, JSON schemas, or structured data with high aggressiveness
