# Quick Start

## Super Simple (Copy/Paste)

Run these 3 commands:

```bash
git clone https://github.com/DevvGwardo/prompt-compress.git
cd prompt-compress
cargo build --release
```

Then run:

```bash
echo "Write a short launch update for my team with 3 bullet points" \
  | ./target/release/compress --stats
```

Use your own prompt:

```bash
echo "YOUR PROMPT HERE" | ./target/release/compress -a 0.4
```

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

See full docs: https://github.com/DevvGwardo/prompt-compress#super-simple-copypaste
