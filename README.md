# PromptCompress

[![Release](https://img.shields.io/badge/release-model--v0.1.0-blue)](https://github.com/DevvGwardo/prompt-compress/releases/tag/model-v0.1.0)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

The prompt optimization layer for LLM apps.

`prompt-compress` removes low-signal words before an LLM call while preserving intent and protected content. It gives you explicit token metrics so you can track cost and latency impact in production.

## Contents

- [What You Get](#what-you-get)
- [Quick Start (60s)](#quick-start-60s)
- [How It Works](#how-it-works)
- [Scoring Modes](#scoring-modes)
- [Codex Integration (Auto-Compress Prompts)](#codex-integration-auto-compress-prompts)
- [OpenClaw Integration (Plugin)](#openclaw-integration-plugin)
- [Wiki Publishing](#wiki-publishing)
- [CLI](#cli)
- [API](#api)
- [Real Benchmark](#real-benchmark)
- [Model Artifacts and Distribution](#model-artifacts-and-distribution)
- [Training Pipeline](#training-pipeline)
- [Development](#development)
- [Repository Layout](#repository-layout)
- [License](#license)

## What You Get

- `compress-core`: embeddable Rust library
- `compress`: CLI for pipelines and local usage
- `compress-api`: HTTP service for centralized compression
- `scripts/codex-compress`: wrapper that compresses prompts before calling Codex
- `integrations/openclaw/prompt-compress`: OpenClaw plugin that applies compression at `before_prompt_build`

## Quick Start (60s)

```bash
cargo build --release
```

```bash
echo "Please summarize the following document in concise bullet points" \
  | ./target/release/compress -a 0.5 --stats
```

Example output:

```text
summarize following document concise bullet points
---
Original tokens:    11
Compressed tokens:  6
Compression ratio:  54.5%
Tokens saved:       5
```

## How It Works

```text
Input text
  -> extract <ttc_safe> regions
  -> score tokens (HeuristicScorer or OnnxScorer)
  -> keep tokens above aggressiveness threshold
  -> rebuild output text
  -> count tokens + return savings metrics
```

Rules:

- `aggressiveness=0.0`: no compression
- `aggressiveness=1.0`: strongest filtering
- `<ttc_safe>...</ttc_safe>`: always preserved

## Scoring Modes

| Mode | Startup | Throughput | Typical Use |
|---|---:|---:|---|
| Heuristic | very low | very high | default production baseline |
| ONNX | higher | lower | semantic scoring experiments / quality tuning |

## Codex Integration (Auto-Compress Prompts)

Use the wrapper script so your prompt is compressed before `codex` runs.

```bash
chmod +x scripts/codex-compress
```

Stdin workflow:

```bash
echo "Review this Rust module for correctness and performance issues" \
  | ./scripts/codex-compress -- exec --full-auto
```

Explicit prompt workflow:

```bash
./scripts/codex-compress \
  --prompt "Draft a migration plan for this repository" \
  -- exec
```

Recommended defaults:

```bash
export PROMPT_COMPRESS_AGGRESSIVENESS=0.4
export PROMPT_COMPRESS_USE_ONNX=0
export PROMPT_COMPRESS_MODEL="$PWD/models"
export PROMPT_COMPRESS_BIN="$PWD/target/release/compress"
```

Optional alias:

```bash
alias codexp="$PWD/scripts/codex-compress"
```

## OpenClaw Integration (Plugin)

This repo includes a ready-to-install OpenClaw plugin package at:

`integrations/openclaw/prompt-compress`

Install and enable:

```bash
openclaw plugins install /absolute/path/to/prompt-compress/integrations/openclaw/prompt-compress
openclaw plugins enable prompt-compress
```

Recommended plugin config:

```json
{
  "plugins": {
    "entries": {
      "prompt-compress": {
        "enabled": true,
        "config": {
          "command": "/absolute/path/to/prompt-compress/target/release/compress",
          "aggressiveness": 0.4,
          "targetModel": "gpt-4",
          "useOnnx": false,
          "modelDir": "/absolute/path/to/prompt-compress/models",
          "minChars": 80,
          "timeoutMs": 2000,
          "onlyIfSmaller": true
        }
      }
    }
  }
}
```

Notes:

- Requires an OpenClaw build that supports `before_prompt_build.promptOverride`.
- The plugin fails open: if compression fails, OpenClaw uses the original prompt.
- Full integration docs: `integrations/openclaw/prompt-compress/README.md`.

## Wiki Publishing

Wiki source pages are tracked in-repo under:

- `wiki/`

Publish script:

```bash
./scripts/publish-wiki.sh
```

If GitHub wiki git remote is not initialized yet, create the first wiki page once in the browser:

- https://github.com/DevvGwardo/prompt-compress/wiki

## CLI

```text
compress [OPTIONS]

-i, --input <TEXT>
-f, --file <PATH>
-a, --aggressiveness <FLOAT>      default: 0.5
-m, --target-model <MODEL>        default: gpt-4
    --onnx
    --model-dir <PATH>            or PROMPT_COMPRESS_MODEL
-s, --stats
    --format <text|json>          default: text
```

Input precedence: `--input` > `--file` > stdin.

## API

Start server:

```bash
cargo run --release -p compress-api
```

With auth:

```bash
COMPRESS_API_KEY=sk-your-secret cargo run --release -p compress-api
```

Endpoints:

- `POST /v1/compress`
- `GET /health`

Request:

```bash
curl -s -X POST http://localhost:3000/v1/compress \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-your-secret" \
  -d '{
    "model": "scorer-v0.1",
    "input": "The quick brown fox jumps over the lazy dog",
    "compression_settings": {
      "aggressiveness": 0.5,
      "target_model": "gpt-4"
    }
  }'
```

Response:

```json
{
  "output": "quick brown fox jumps lazy dog",
  "output_tokens": 6,
  "original_input_tokens": 9,
  "compression_ratio": 0.6666666666666666
}
```

Note: request field `model` is currently accepted but not used to select scorer mode by the API service.

## Real Benchmark

Built-in benchmark harness:

```bash
cargo run --release -p compress-core --example benchmark -- \
  --dataset training/corpus.txt \
  --samples 500 \
  --warmup 25 \
  --aggressiveness 0.5 \
  --mode both
```

Measured on 2026-03-04 (`Darwin arm64`, `hw.model=Mac16,8`, 12 cores):

| Mode | Load (ms) | p50 (ms) | p95 (ms) | p99 (ms) | Mean (ms) | Prompts/s | Input tok/s | Tokens In -> Out | Ratio | Saved |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Heuristic | 31.0 | 0.05 | 0.11 | 0.14 | 0.06 | 16732.9 | 854450 | 25532 -> 19684 | 0.771 | 22.9% |
| ONNX | 186.7 | 26.35 | 27.45 | 31.92 | 26.55 | 37.7 | 1924 | 25532 -> 22927 | 0.898 | 10.2% |

## Model Artifacts and Distribution

When `--onnx` is enabled, model resolution order is:

1. `PROMPT_COMPRESS_MODEL`
2. `./models`
3. `models/scorer-v0.1`
4. `../models`
5. `../../models`

Required files in model directory:

- `model.onnx`
- `tokenizer.json`

Large ONNX files can exceed normal GitHub limits. Recommended options:

1. Git LFS for repository-managed binaries
2. GitHub Release assets for distribution-only binaries

Current release asset example:

- https://github.com/DevvGwardo/prompt-compress/releases/tag/model-v0.1.0

## Training Pipeline

```bash
cd training
pip install -r requirements.txt

# Label dataset
python prepare_dataset.py --input corpus.txt --output data.jsonl --method heuristic
python prepare_dataset.py --input corpus.txt --output data_claude.jsonl --method claude-cli

# Train + export
python train.py --data data.jsonl --output ./output --epochs 3 --batch-size 16 --lr 5e-5
python export_onnx.py --model-dir ./output/best --output ../models/model.onnx

# Evaluate
python evaluate.py --model-dir ./output/best --data data_claude_test.jsonl --show-samples 10
```

## Development

```bash
cargo check --all
cargo clippy --all
cargo test --all
cargo build --release
```

## Repository Layout

```text
prompt-compress/
├── crates/
│   ├── compress-core/
│   ├── compress-cli/
│   └── compress-api/
├── integrations/
│   └── openclaw/
│       └── prompt-compress/
├── wiki/
├── scripts/
│   └── codex-compress
├── training/
└── models/
```

## License

MIT
