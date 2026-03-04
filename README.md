<p align="center">
  <h1 align="center">prompt-compress</h1>
  <p align="center">
    <strong>LLM prompt compression tool — score token importance and strip the fat before sending to any model.</strong>
  </p>
  <p align="center">
    <a href="#quickstart">Quickstart</a> &nbsp;&bull;&nbsp;
    <a href="#api-reference">API</a> &nbsp;&bull;&nbsp;
    <a href="#cli-reference">CLI</a> &nbsp;&bull;&nbsp;
    <a href="#how-it-works">How It Works</a> &nbsp;&bull;&nbsp;
    <a href="#training-pipeline">Training</a>
  </p>
</p>

---

Feed in a long prompt, get back a shorter one that preserves meaning. Save tokens, save money.

prompt-compress scores every word's importance using a pluggable scorer (heuristic today, fine-tuned DistilBERT via ONNX coming soon), drops the low-value ones, and returns compressed text with token count stats. Ships as a **REST API** and a **CLI tool**, both built in Rust.

```
$ echo "The quick brown fox jumps over the lazy dog and it was a very good day" \
    | compress -a 0.5 --stats

quick brown fox jumps lazy dog good day
---
Original tokens:    16
Compressed tokens:  8
Compression ratio:  50.0%
Tokens saved:       8
```

## Live Comparison: Original vs Compressed with Claude

We sent a real code review prompt (227 tokens) through `compress` at different aggressiveness levels, then sent both original and compressed versions to Claude Opus 4.6 to compare output quality.

### The Prompt

> *"You are an expert software engineer. I need you to review the following code and provide specific, actionable feedback on how to improve it. Focus on performance, readability, and potential bugs..."* (full Python code review prompt with a `process_user_data` function)

### Compression Results

| | Tokens | Ratio | Saved |
|---|:---:|:---:|:---:|
| **Original** | 227 | 100% | — |
| **Compressed (0.3)** | 156 | 69% | **31% fewer tokens** |
| **Compressed (0.5)** | 156 | 69% | **31% fewer tokens** |
| **Compressed (0.7)** | 83 | 37% | **63% fewer tokens** |

### Quality Comparison

<table>
<tr><th></th><th>Original (227 tokens)</th><th>Compressed @ 0.3 (156 tokens)</th></tr>
<tr>
<td><strong>Issues Found</strong></td>
<td>5 issues: range(len), no error handling, missing purchases key, string concat, could use comprehension</td>
<td>6 issues: range(len), missing keys, no input validation, missing purchases, fragile name construction, no type hints</td>
</tr>
<tr>
<td><strong>Fix Quality</strong></td>
<td>List comprehension, .get() with defaults, f-string</td>
<td>Early continue pattern, .get() with defaults, f-string, type hints, .strip() on names</td>
</tr>
<tr>
<td><strong>Rating Given</strong></td>
<td>6/10</td>
<td>4/10 (with detailed rubric table)</td>
</tr>
<tr>
<td><strong>Verdict</strong></td>
<td>Solid review</td>
<td>More thorough review — found more issues, provided a scoring rubric</td>
</tr>
</table>

> **Result:** At 0.3 aggressiveness (31% token savings), Claude produced an **equally good or better** response. The compressed prompt preserved all semantic meaning while stripping grammatical filler.

> **Quality cliff:** At 0.7 aggressiveness (63% savings), Claude started hallucinating bugs that didn't exist in the original code — too much context was lost. **The sweet spot is 0.3–0.5** for code-related prompts.

### Same Prompt Through Kimi (kimi-cli)

We ran the identical prompt and compression levels through [Kimi](https://kimi.ai) to see how a different model handles compressed input.

#### Compression Results

| | Tokens | Ratio | Saved |
|---|:---:|:---:|:---:|
| **Original** | 172 | 100% | — |
| **Compressed (0.3)** | 119 | 69% | **31% fewer tokens** |
| **Compressed (0.7)** | 57 | 33% | **67% fewer tokens** |

#### Quality Comparison

<table>
<tr><th></th><th>Original (172 tokens)</th><th>Compressed @ 0.3 (119 tokens)</th><th>Compressed @ 0.7 (57 tokens)</th></tr>
<tr>
<td><strong>Issues Found</strong></td>
<td>6 issues: range(len), wasted computation, no error handling, no null safety, verbose accumulation, missing type safety</td>
<td>Misread compression as syntax errors — found 8 "critical syntax errors" (missing <code>=</code>, <code>for</code>, <code>if</code>, <code>in</code>) plus real issues</td>
<td>Had to reconstruct the likely code from fragments — lost the review framing entirely</td>
</tr>
<tr>
<td><strong>Fix Quality</strong></td>
<td>.get() with defaults, f-strings, sum() generator, input validation, type hints, docstring</td>
<td>Same fixes as original — still got .get(), f-strings, sum(), type hints — but prefaced with syntax error corrections</td>
<td>Provided corrected code but rated its own improvements (9/10) instead of the original — misunderstood the ask</td>
</tr>
<tr>
<td><strong>Rating Given</strong></td>
<td>4/10 (with per-category breakdown)</td>
<td>2/10 (penalized for "syntax errors" that were compression artifacts)</td>
<td>9/10 (rated the improved code, not the original — confused by lost context)</td>
</tr>
<tr>
<td><strong>Verdict</strong></td>
<td>Thorough, accurate review</td>
<td>Correct fixes but confused by compression artifacts</td>
<td>Lost the task framing entirely</td>
</tr>
</table>

#### Claude vs Kimi: Key Takeaways

| | Claude (Opus 4.6) | Kimi |
|---|---|---|
| **Original prompt** | 5 issues, 6/10 rating | 6 issues, 4/10 rating |
| **Compressed @ 0.3** | Equally good or better — found *more* issues, added scoring rubric | Misinterpreted compression artifacts as code syntax errors |
| **Compressed @ 0.7** | Started hallucinating non-existent bugs | Lost the task framing, rated its own fixes instead |
| **Compression tolerance** | Handles compressed text gracefully up to ~0.5 | Struggles with compressed text at all levels — treats missing words as broken code |

> **Finding:** Claude handles compressed prompts significantly better than Kimi. Claude appears to "fill in the blanks" and infer intent from compressed text, while Kimi interprets the missing grammar literally. **If you're compressing prompts for Kimi, use very light compression (0.1–0.2) or protect the code block with `<ttc_safe>` tags.**

## Features

- **Token-level importance scoring** with a `TokenScorer` trait — swap heuristic for ML with zero API changes
- **`<ttc_safe>` tags** to protect critical text regions from compression
- **REST API** (`POST /v1/compress`) compatible with [The Token Company](https://thetokencompany.com) schema
- **CLI** with stdin/file/inline input, text or JSON output
- **Accurate LLM token counting** via `tiktoken-rs` (cl100k_base — GPT-4 / Claude)
- **Bearer auth, gzip, tracing** middleware out of the box
- **Python training pipeline** to fine-tune DistilBERT and export to ONNX

## Quickstart

### Prerequisites

- Rust 1.75+ and Cargo
- Python 3.10+ (only for the training pipeline)

### Build

```bash
git clone https://github.com/DevvGwardo/prompt-compress.git
cd prompt-compress
cargo build --release
```

Binaries land in `target/release/`:
- `compress` — CLI tool
- `compress-api` — API server

### CLI

```bash
# Pipe text
echo "Please summarize the following document for me" | ./target/release/compress -a 0.5

# From a file
./target/release/compress -f prompt.txt -a 0.7 --stats

# Inline text
./target/release/compress -i "Remove all the unnecessary words from this sentence" -a 0.6

# JSON output
echo "long prompt here" | ./target/release/compress -a 0.5 --format json
```

### API server

```bash
# Start without auth
cargo run --release -p compress-api

# Start with auth
COMPRESS_API_KEY=sk-your-secret cargo run --release -p compress-api

# Custom port
PORT=8080 cargo run --release -p compress-api
```

```bash
curl -s -X POST http://localhost:3000/v1/compress \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-your-secret" \
  -d '{
    "input": "The quick brown fox jumps over the lazy dog",
    "compression_settings": { "aggressiveness": 0.5 }
  }' | jq
```

```json
{
  "output": "quick brown fox jumps lazy dog",
  "output_tokens": 6,
  "original_input_tokens": 9,
  "compression_ratio": 0.6666666666666666
}
```

## How It Works

```
Input text
  │
  ▼
┌─────────────────────────────────────┐
│  1. Extract <ttc_safe> regions      │  Protected words always survive
│  2. Score each word (0.0 – 1.0)     │  HeuristicScorer or OnnxScorer
│  3. Filter: keep if score ≥ thresh  │  threshold = aggressiveness
│  4. Reconstruct compressed text     │  Space-joined kept words
│  5. Count tokens (tiktoken)         │  cl100k_base BPE
└─────────────────────────────────────┘
  │
  ▼
Compressed output + token stats
```

### Aggressiveness

Controls how many tokens get cut. Higher = more aggressive compression.

| Aggressiveness | Threshold | Behavior |
|:---:|:---:|---|
| `0.0` | 0.0 | No compression — returns input unchanged |
| `0.3` | 0.3 | Light — removes only the most obvious filler |
| `0.5` | 0.5 | Moderate — good default for most prompts |
| `0.7` | 0.7 | Heavy — keeps mainly content-carrying words |
| `0.9` | 0.9 | Aggressive — only the most important words survive |
| `1.0` | 1.0 | Maximum — only perfect-score tokens remain |

### Heuristic Scoring Rules

The built-in `HeuristicScorer` assigns importance based on word characteristics:

| Pattern | Score | Rationale |
|---|:---:|---|
| ALL CAPS (`API`, `HTTP`) | 0.95 | Acronyms, emphasis |
| Capitalized (`John`, `Monday`) | 0.90 | Proper nouns |
| Numbers (`42.99`, `2024`) | 0.80 | Data values |
| Long words (12+ chars) | 0.50–0.70 | Semantic density scales with length |
| Regular words | 0.50–0.70 | Base + length bonus |
| Short words (1–2 chars) | 0.30 | Low information |
| Stop words (`the`, `is`, `of`) | 0.20 | Grammatical filler |
| Punctuation | 0.15 | Minimal information |

### Safe Tags

Wrap critical text in `<ttc_safe>` tags to guarantee it survives any aggressiveness level:

```
Compress this: <ttc_safe>user_id = 42</ttc_safe> and remove the rest of the filler.
```

At aggressiveness `0.9`, the protected words `user_id`, `=`, `42` are always kept. Multiple safe regions are supported.

## Project Structure

```
prompt-compress/
├── Cargo.toml                        # Workspace root
├── crates/
│   ├── compress-core/                # Shared library
│   │   └── src/
│   │       ├── lib.rs                # Public API re-exports
│   │       ├── scorer.rs             # TokenScorer trait + HeuristicScorer
│   │       ├── compressor.rs         # Orchestrator: score → filter → reconstruct
│   │       ├── tokenizer.rs          # tiktoken wrapper (LlmTokenCounter)
│   │       ├── model.rs              # OnnxScorer (Phase 3 placeholder)
│   │       ├── config.rs             # CompressionSettings, CompressionResult
│   │       └── error.rs              # CompressError enum
│   ├── compress-api/                 # REST API server
│   │   ├── src/
│   │   │   ├── main.rs              # Server startup + middleware stack
│   │   │   ├── routes.rs            # POST /v1/compress, GET /health
│   │   │   ├── middleware.rs         # Bearer token auth
│   │   │   ├── state.rs             # AppState
│   │   │   └── dto.rs               # Request/response types
│   │   └── tests/
│   │       └── api_test.rs           # Integration tests (5 tests)
│   └── compress-cli/                 # CLI tool
│       └── src/main.rs               # clap-based CLI
├── training/                         # Python ML pipeline
│   ├── requirements.txt
│   ├── prepare_dataset.py            # Label generation (heuristic / Claude)
│   ├── train.py                      # Fine-tune DistilBERT
│   ├── export_onnx.py                # PyTorch → ONNX + INT8 quantization
│   └── evaluate.py                   # Model evaluation
└── models/                           # ONNX model artifacts (gitignored)
```

## API Reference

### `POST /v1/compress`

Compress input text. Protected by bearer auth if `COMPRESS_API_KEY` is set.

**Request:**

```json
{
  "model": "scorer-v0.1",
  "input": "your text here",
  "compression_settings": {
    "aggressiveness": 0.5,
    "target_model": "gpt-4"
  }
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `model` | string | `"scorer-v0.1"` | Scorer model identifier |
| `input` | string | *required* | Text to compress |
| `compression_settings.aggressiveness` | float | `0.5` | 0.0–1.0 compression level |
| `compression_settings.target_model` | string | `"gpt-4"` | Target LLM for token counting |

**Response (200):**

```json
{
  "output": "compressed text",
  "output_tokens": 5,
  "original_input_tokens": 12,
  "compression_ratio": 0.4166
}
```

**Error (400):**

```json
{
  "error": {
    "message": "input is empty",
    "type": "invalid_request_error"
  }
}
```

**Auth error (401):** Empty body, status 401.

### `GET /health`

Returns `ok` (plain text, no auth required).

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `COMPRESS_API_KEY` | *(unset)* | Bearer token for auth. If unset, auth is disabled. |
| `PORT` | `3000` | Server listen port |
| `RUST_LOG` | — | Tracing filter (e.g. `compress_api=debug`) |

### Middleware Stack

| Layer | Description |
|---|---|
| `CompressionLayer` | Gzip-compresses response bodies |
| `RequestDecompressionLayer` | Decompresses gzip request bodies |
| `TraceLayer` | Logs HTTP method, path, status, duration |
| Bearer Auth | Validates `Authorization: Bearer <token>` (API routes only) |

## CLI Reference

```
compress [OPTIONS]
```

| Flag | Short | Default | Description |
|---|---|---|---|
| `--input <TEXT>` | `-i` | — | Inline input text |
| `--file <PATH>` | `-f` | — | Read input from file |
| `--aggressiveness <FLOAT>` | `-a` | `0.5` | Compression level (0.0–1.0) |
| `--target-model <MODEL>` | `-m` | `gpt-4` | LLM for token counting |
| `--stats` | `-s` | off | Print compression statistics to stderr |
| `--format <FORMAT>` | — | `text` | Output format: `text` or `json` |

**Input priority:** `-i` > `-f` > stdin

**Examples:**

```bash
# Moderate compression with stats
echo "your long prompt" | compress -a 0.5 --stats

# Heavy compression, JSON output
compress -f prompt.txt -a 0.8 --format json

# Protect critical text
compress -i "drop filler but <ttc_safe>keep this exact phrase</ttc_safe>" -a 0.9
```

## Training Pipeline

The `training/` directory contains a complete pipeline to replace the heuristic scorer with a fine-tuned DistilBERT model.

### 1. Prepare Dataset

Generate token-level keep/discard labels:

```bash
cd training
pip install -r requirements.txt

# Heuristic labeling (no API key needed)
python prepare_dataset.py -i corpus.txt -o labels.jsonl --method heuristic

# Claude-distilled labels (requires ANTHROPIC_API_KEY — not yet implemented)
python prepare_dataset.py -i corpus.txt -o labels.jsonl --method claude
```

Output format (JSONL):
```json
{"text": "the quick brown fox", "words": ["the", "quick", "brown", "fox"], "labels": [0, 1, 1, 1]}
```

### 2. Fine-tune DistilBERT

```bash
python train.py --data labels.jsonl --output ./output --epochs 3 --batch-size 16 --lr 5e-5
```

Trains `distilbert-base-uncased` (66M params) for token classification. Best checkpoint saved to `./output/best/`.

### 3. Export to ONNX

```bash
# Optimized only
python export_onnx.py --model-dir ./output/best --output ../models/scorer-v0.1.onnx

# Optimized + INT8 quantized
python export_onnx.py --model-dir ./output/best --output ../models/scorer-v0.1.onnx --quantize
```

### 4. Evaluate

```bash
python evaluate.py --model-dir ./output/best --data test.jsonl --show-samples 10
```

Prints accuracy, F1, precision, recall, and sample compressions.

## Architecture

The codebase is designed around a **`TokenScorer` trait** that decouples scoring strategy from the compression pipeline:

```rust
pub trait TokenScorer: Send + Sync {
    fn score(&self, text: &str) -> Result<Vec<ScoredToken>>;
}
```

Two implementations:

| Scorer | Status | Latency | Quality |
|---|---|---|---|
| `HeuristicScorer` | Production-ready | <1ms | Good for stop-word removal |
| `OnnxScorer` | Placeholder (Phase 3) | ~10-20ms target | ML-grade semantic scoring |

The `Compressor` accepts any `Box<dyn TokenScorer>`, so swapping heuristic for ONNX is a one-line change:

```rust
// Heuristic (current)
let scorer = HeuristicScorer::new();

// ONNX (after training)
let scorer = OnnxScorer::load("models/scorer-v0.1.onnx")?;

let compressor = Compressor::new(Box::new(scorer), "gpt-4")?;
```

### Key Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `ort` | 2.0.0-rc.11 | ONNX Runtime inference |
| `axum` | 0.8 | HTTP framework |
| `clap` | 4 | CLI argument parsing |
| `tiktoken-rs` | 0.9 | LLM token counting (cl100k_base) |
| `tokenizers` | 0.22 | HuggingFace WordPiece tokenizer |
| `tower-http` | 0.6 | Gzip, tracing, CORS middleware |

## Testing

```bash
# Run all 128 tests
cargo test

# Core library only
cargo test -p compress-core

# API integration tests only
cargo test -p compress-api

# CLI integration tests only
cargo test -p compress-cli
```

### Test Coverage

| Module | Tests | Covers |
|---|---|---|
| `scorer` | 22 | Stop words, punctuation, numbers, capitalization, ALL CAPS, length bonus, unicode, whitespace, mixed content ordering |
| `compressor` | 32 | Aggressiveness spectrum (0.0–1.0), safe tags (nested/empty/multiple/removed), error handling, determinism, long input, unicode, reusability |
| `config` | 10 | Default values, serde serialization/deserialization, JSON roundtripping, partial JSON defaults, Clone |
| `error` | 9 | All error variant display messages, anyhow conversion, Send + Sync + Debug trait bounds |
| `tokenizer` | 14 | Token counting, known counts, unicode, special characters, determinism, reusability, model names |
| `model` | 4 | Missing model error, path in error message, fake file fallback, scorer fallback |
| `api` | 21 | Schema validation, aggressiveness levels, safe tags, auth edge cases (empty bearer, Basic scheme), unicode, long input, sequential requests, error responses |
| `cli` | 16 | All input modes (stdin/file/inline), JSON/text formats, --stats, aggressiveness, safe tags, error cases, --help, --version, target model |

## Roadmap

- [x] **Phase 0:** Cargo workspace + heuristic scorer + CLI
- [x] **Phase 1:** REST API with axum (auth, gzip, tracing)
- [x] **Phase 2:** Python training pipeline (DistilBERT fine-tuning + ONNX export)
- [ ] **Phase 3:** ONNX model integration in Rust (`OnnxScorer` with sliding window, sub-word aggregation)
- [ ] **Phase 4:** Rate limiting, model versioning, Docker build, load testing

## License

MIT
