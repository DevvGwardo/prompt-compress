# PromptCompress

[![Release](https://img.shields.io/badge/release-model--v0.1.0-blue)](https://github.com/DevvGwardo/prompt-compress/releases/tag/model-v0.1.0)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

`prompt-compress` is a Rust toolkit for reducing prompt token count before LLM calls while preserving critical content.

It includes:

- `compress-core`: embeddable library
- `compress` (CLI): local and pipeline usage
- `compress-api`: HTTP service
- `scripts/codex-compress`: Codex wrapper with pre-send compression
- `integrations/openclaw/prompt-compress`: OpenClaw plugin package

## Why Use It

- Lower input token volume and cost
- Keep important terms protected via `<ttc_safe>...</ttc_safe>`
- Choose fast heuristic scoring or ONNX scoring
- Get explicit token metrics for observability
- Deploy as library, CLI, or API

## Quick Start

Build binaries:

```bash
cargo build --release
```

Run compression from stdin:

```bash
echo "Please summarize the following document in concise bullet points" \
  | ./target/release/compress --aggressiveness 0.5 --stats
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

## Architecture

```text
input text
  -> extract <ttc_safe> regions
  -> score tokens (HeuristicScorer or OnnxScorer)
  -> keep tokens above aggressiveness threshold
  -> rebuild output text
  -> report token savings
```

Compression rules:

- `aggressiveness=0.0`: no compression
- `aggressiveness=1.0`: strongest filtering
- `<ttc_safe>...</ttc_safe>` content is preserved

## Scoring Modes

| Mode | Startup Cost | Throughput | Typical Use |
|---|---:|---:|---|
| Heuristic | Very low | Very high | Default production baseline |
| ONNX | Higher | Lower | Semantic scoring experiments and tuning |

## CLI

Command:

```text
compress [OPTIONS]
```

Key options:

- `-i, --input <TEXT>`: direct input
- `-f, --file <PATH>`: read prompt from file
- `-a, --aggressiveness <FLOAT>`: `0.0..1.0` (default `0.5`)
- `-m, --target-model <MODEL>`: tokenizer model (default `gpt-4`)
- `--onnx`: use ONNX scorer
- `--model-dir <PATH>`: ONNX model directory (or `PROMPT_COMPRESS_MODEL`)
- `-s, --stats`: print token stats to stderr
- `--format <text|json>`: output format (default `text`)

Input precedence: `--input` > `--file` > stdin.

JSON output example:

```bash
echo "Analyze this codebase for correctness risks" \
  | ./target/release/compress --format json
```

## API

Start server:

```bash
cargo run --release -p compress-api
```

Enable bearer auth:

```bash
COMPRESS_API_KEY=sk-your-secret cargo run --release -p compress-api
```

Endpoints:

- `POST /v1/compress`
- `GET /health`

Request example:

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

Response example:

```json
{
  "output": "quick brown fox jumps lazy dog",
  "output_tokens": 6,
  "original_input_tokens": 9,
  "compression_ratio": 0.6666666666666666
}
```

Note: `compress-api` currently supports `model` values `scorer-v0.1` and `heuristic-v0.1`.

## Codex Integration

Use the wrapper script to compress prompts before sending them to `codex`.

```bash
chmod +x scripts/codex-compress
```

Stdin flow:

```bash
echo "Review this Rust module for correctness and performance issues" \
  | ./scripts/codex-compress -- exec --full-auto
```

Explicit prompt flow:

```bash
./scripts/codex-compress \
  --prompt "Draft a migration plan for this repository" \
  -- exec
```

Recommended environment defaults:

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

## OpenClaw Integration

Plugin package location:

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

Integration notes:

- Requires OpenClaw support for `before_prompt_build.promptOverride`
- Plugin fails open (original prompt is used if compression fails)
- See `integrations/openclaw/prompt-compress/README.md` for integration detail

## Performance

Benchmark harness:

```bash
cargo run --release -p compress-core --example benchmark -- \
  --dataset training/corpus.txt \
  --samples 500 \
  --warmup 25 \
  --aggressiveness 0.5 \
  --mode both
```

Reference run from `2026-03-04` on `Darwin arm64` (`Mac16,8`, 12 cores):

| Mode | Load (ms) | p50 (ms) | p95 (ms) | p99 (ms) | Mean (ms) | Prompts/s | Input tok/s | Tokens In -> Out | Ratio | Saved |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Heuristic | 31.0 | 0.05 | 0.11 | 0.14 | 0.06 | 16732.9 | 854450 | 25532 -> 19684 | 0.771 | 22.9% |
| ONNX | 186.7 | 26.35 | 27.45 | 31.92 | 26.55 | 37.7 | 1924 | 25532 -> 22927 | 0.898 | 10.2% |

## ONNX Model Artifacts

When `--onnx` is enabled, model directory resolution order is:

1. `PROMPT_COMPRESS_MODEL`
2. `./models`
3. `models/scorer-v0.1`
4. `../models`
5. `../../models`

Required files:

- `model.onnx`
- `tokenizer.json`

For large binaries, prefer:

1. Git LFS for repo-managed artifacts
2. GitHub Releases for distribution artifacts

Example release:

- https://github.com/DevvGwardo/prompt-compress/releases/tag/model-v0.1.0

## Training

```bash
cd training
pip install -r requirements.txt

# Label dataset
python prepare_dataset.py --input corpus.txt --output data.jsonl --method heuristic
python prepare_dataset.py --input corpus.txt --output data_claude.jsonl --method claude-cli

# Train and export
python train.py --data data.jsonl --output ./output --epochs 3 --batch-size 16 --lr 5e-5
python export_onnx.py --model-dir ./output/best --output ../models/model.onnx

# Evaluate
python evaluate.py --model-dir ./output/best --data data_claude_test.jsonl --show-samples 10
```

## Documentation

- Wiki pages are source-controlled in `wiki/` and published to GitHub Wiki
- Wiki maintenance and publishing workflow: [CONTRIBUTING.md](CONTRIBUTING.md)
- Wiki landing page: https://github.com/DevvGwardo/prompt-compress/wiki

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
├── scripts/
│   ├── codex-compress
│   └── publish-wiki.sh
├── training/
├── wiki/
└── models/
```

## License

MIT
