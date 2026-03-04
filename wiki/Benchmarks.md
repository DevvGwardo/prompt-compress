# Benchmarks

Run benchmark harness:

```bash
cargo run --release -p compress-core --example benchmark -- \
  --dataset training/corpus.txt \
  --samples 500 \
  --warmup 25 \
  --aggressiveness 0.5 \
  --mode both
```

Reference measurement (2026-03-04, Darwin arm64, Mac16,8):

| Mode | Load (ms) | p50 (ms) | p95 (ms) | p99 (ms) | Mean (ms) | Prompts/s | Input tok/s | Tokens In -> Out | Ratio | Saved |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Heuristic | 31.0 | 0.05 | 0.11 | 0.14 | 0.06 | 16732.9 | 854450 | 25532 -> 19684 | 0.771 | 22.9% |
| ONNX | 186.7 | 26.35 | 27.45 | 31.92 | 26.55 | 37.7 | 1924 | 25532 -> 22927 | 0.898 | 10.2% |

See latest benchmark section in README for updates:
- https://github.com/DevvGwardo/prompt-compress#real-benchmark
