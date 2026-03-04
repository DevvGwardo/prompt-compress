# Quick Start

Build:

```bash
cargo build --release
```

Run compression from stdin:

```bash
echo "Please summarize the following document in concise bullet points" \
  | ./target/release/compress -a 0.5 --stats
```

JSON output:

```bash
echo "Analyze this codebase for critical bugs" \
  | ./target/release/compress --format json
```

Read from file:

```bash
./target/release/compress --file prompt.txt --aggressiveness 0.4
```

See full docs: https://github.com/DevvGwardo/prompt-compress#quick-start-60s
