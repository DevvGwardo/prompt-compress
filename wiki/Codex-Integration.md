# Codex Integration

Use the included scripts to compress prompts before or during `codex` runs.

Script paths:

- `scripts/codex-compress`
- `scripts/codex-chat-compress`
- `scripts/codex-proxy` (compatibility shim)

Example with stdin:

```bash
echo "Review this Rust module for correctness and performance issues" \
  | ./scripts/codex-compress -- exec --full-auto
```

Example with explicit prompt:

```bash
./scripts/codex-compress \
  --prompt "Draft a migration plan for this repository" \
  -- exec
```

Plain interactive launch (initial prompt compression):

```bash
./scripts/codex-compress
```

This prompts for an initial message, compresses it, then launches interactive `codex`.

Per-turn interactive launch (recommended):

```bash
./scripts/codex-chat-compress
```

This wrapper runs a local prompt loop and sends each turn through `codex exec`
or `codex exec resume`. It tracks the active Codex thread for the current
directory and compresses each prompt before sending it.

Recommended environment:

```bash
export PROMPT_COMPRESS_AGGRESSIVENESS=0.4
export PROMPT_COMPRESS_USE_ONNX=0
export PROMPT_COMPRESS_MODEL="$PWD/models"
export PROMPT_COMPRESS_BIN="$PWD/target/release/compress"
export PROMPT_COMPRESS_CODEX_BIN="/absolute/path/to/real/codex"
```

Optional alias:

```bash
alias codex="$PWD/scripts/codex-chat-compress"
alias codex-native="/opt/homebrew/bin/codex"
alias codex-oneshot="$PWD/scripts/codex-compress"
```

Notes:

- `scripts/codex-compress` only affects the initial message.
- `scripts/codex-chat-compress` handles follow-up turns too.
- Native interactive Codex does not currently honor the proxy hook for real turn traffic on ChatGPT-plan builds, so the `codex exec` wrapper is the reliable path.
- Savings are logged to `/tmp/prompt-compress-codex-chat.log`, for example:

```text
2026-03-06T02:34:12Z codex-chat stats cwd=/Users/devgwardo/prompt-compress session=019... original=1280 compressed=914 sent=914 saved=366 ratio=28.6% rewritten=1
```

See also: https://github.com/DevvGwardo/prompt-compress#codex-integration
