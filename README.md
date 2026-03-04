# prompt-compress

Production-ready prompt compression for LLM apps, agents, and APIs.

`prompt-compress` removes low-value words while preserving intent, protected content, and measurable token savings.

It ships as:

- `compress-core`: reusable Rust library
- `compress`: CLI for local and pipeline use
- `compress-api`: HTTP API for service deployments

## How It Works (30 Seconds)

1. Parse input text and extract `<ttc_safe>...</ttc_safe>` regions.
2. Score each token with either:
   - `HeuristicScorer` for fast, dependency-light compression
   - `OnnxScorer` for ML-based semantic scoring
3. Keep tokens at or above the `aggressiveness` threshold.
4. Reconstruct compressed text while preserving protected regions.
5. Return compressed output plus token metrics (`output_tokens`, ratio, savings).

```text
Input -> Safe-tag extraction -> Token scoring -> Threshold filter -> Rebuild text -> Token stats
```

## Why Teams Use It

- Reduce token cost and latency before every LLM call
- Protect critical text (`IDs`, code fragments, policy text) with safe tags
- Keep integration simple with a library, CLI, or REST API

## Quick Example

```bash
echo "Please summarize the following document for me in a concise format" \
  | ./target/release/compress -a 0.5 --stats
```

```text
summarize following document concise format
---
Original tokens:    13
Compressed tokens:  5
Compression ratio:  38.5%
Tokens saved:       8
```

## Highlights

- Pluggable scoring via `TokenScorer`:
  - `HeuristicScorer` (fast, no model files)
  - `OnnxScorer` (ML-based scoring via ONNX Runtime)
- Safe regions with `<ttc_safe>...</ttc_safe>` so critical text is always preserved
- Token accounting using `tiktoken-rs`
- CLI output in plain text or JSON
- API middleware for auth, gzip compression/decompression, and request tracing

## Repository Layout

```text
prompt-compress/
├── crates/
│   ├── compress-core/   # Core compression logic + scorer implementations
│   ├── compress-cli/    # `compress` binary
│   └── compress-api/    # `compress-api` HTTP server (Axum)
├── training/            # Dataset prep, training, ONNX export, evaluation
└── models/              # ONNX artifacts (optional at runtime)
```

## Prerequisites

- Rust 1.75+
- Cargo
- Python 3.10+ (only needed for training/export pipeline)

## Build

```bash
cargo build --release
```

Binaries:

- `target/release/compress`
- `target/release/compress-api`

## CLI Usage

### Basic examples

```bash
# stdin
echo "The quick brown fox jumps over the lazy dog" | ./target/release/compress -a 0.5

# file input
./target/release/compress -f prompt.txt -a 0.6 --stats

# inline input
./target/release/compress -i "Please summarize this in one paragraph" -a 0.5

# JSON output
./target/release/compress -i "long prompt here" --format json
```

### ONNX scoring

```bash
# Auto-discover model directory (see Model Resolution)
./target/release/compress --onnx -f prompt.txt

# Explicit model directory
./target/release/compress --onnx --model-dir ./models -f prompt.txt

# Equivalent via environment variable
PROMPT_COMPRESS_MODEL=./models ./target/release/compress --onnx -f prompt.txt
```

### CLI options

```text
compress [OPTIONS]

-i, --input <TEXT>
-f, --file <PATH>
-a, --aggressiveness <FLOAT>   (default: 0.5)
-m, --target-model <MODEL>     (default: gpt-4)
    --onnx
    --model-dir <PATH>         (or PROMPT_COMPRESS_MODEL)
-s, --stats
    --format <text|json>       (default: text)
```

Input precedence: `--input` > `--file` > stdin.

## API Usage

### Start server

```bash
# No auth
cargo run --release -p compress-api

# Require bearer token
COMPRESS_API_KEY=sk-your-secret cargo run --release -p compress-api

# Custom port
PORT=8080 cargo run --release -p compress-api
```

### Endpoints

- `POST /v1/compress`
- `GET /health`

### Request example

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

### Response example

```json
{
  "output": "quick brown fox jumps lazy dog",
  "output_tokens": 6,
  "original_input_tokens": 9,
  "compression_ratio": 0.6666666666666666
}
```

Note: `model` is currently accepted in the request schema but not used for scorer selection by the API server.

## Compression Behavior

- `aggressiveness = 0.0`: returns input unchanged
- `aggressiveness = 1.0`: keeps only highest-scoring tokens
- `<ttc_safe>...</ttc_safe>` regions are always preserved

Example:

```text
Input:  Compress this <ttc_safe>user_id = 42</ttc_safe> aggressively
Output: compress user_id = 42 aggressively
```

## Model Resolution

When `--onnx` is enabled, model discovery follows:

1. `PROMPT_COMPRESS_MODEL`
2. `./models`
3. `models/scorer-v0.1`
4. `../models`
5. `../../models`

A valid model directory must contain:

- `model.onnx`
- `tokenizer.json`

## Large Model Files (Git LFS / Release Assets)

ONNX binaries are typically larger than GitHub's normal file-size limit. Use one of these approaches before pushing:

1. Git LFS (tracked in this repo via `.gitattributes`)
2. GitHub Release assets (preferred if you do not want large model history in git)

### Git LFS quick setup

```bash
git lfs install --local
git lfs track "models/*.onnx"
git lfs track "models/**/*.onnx"
git add .gitattributes
```

### Release asset workflow (alternative)

1. Keep model binaries out of normal commits.
2. Create a tagged release.
3. Upload `model.onnx` and `tokenizer.json` as release assets.
4. Download assets during deployment/startup as needed.

## Training Pipeline

The `training/` folder provides end-to-end model preparation:

```bash
cd training
pip install -r requirements.txt

# Create labeled dataset
python prepare_dataset.py --input corpus.txt --output data.jsonl --method heuristic
python prepare_dataset.py --input corpus.txt --output data_claude.jsonl --method claude-cli

# Train
python train.py --data data.jsonl --output ./output --epochs 3 --batch-size 16 --lr 5e-5

# Export ONNX
python export_onnx.py --model-dir ./output/best --output ../models/model.onnx

# Evaluate
python evaluate.py --model-dir ./output/best --data data_claude_test.jsonl --show-samples 10
```

## Quality Checks

```bash
cargo check --all
cargo clippy --all
cargo test --all
cargo build --release
```

## License

MIT
