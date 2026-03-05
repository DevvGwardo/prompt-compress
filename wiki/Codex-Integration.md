# Codex Integration

Use the included scripts to compress prompts before or during `codex` runs.

Script paths:

- `scripts/codex-compress`
- `scripts/codex-proxy`

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
./scripts/codex-proxy
```

This launcher starts `compress-api` locally if needed, routes Codex through the
proxy using `chatgpt_base_url`, and evaluates every non-empty prompt block for
compression during the session.

Recommended environment:

```bash
export PROMPT_COMPRESS_AGGRESSIVENESS=0.4
export PROMPT_COMPRESS_USE_ONNX=0
export PROMPT_COMPRESS_MODEL="$PWD/models"
export PROMPT_COMPRESS_BIN="$PWD/target/release/compress"
export PROMPT_COMPRESS_INTERACTIVE_FIRST_PROMPT=1
export COMPRESS_PROXY_AGGRESSIVENESS=0.4
export COMPRESS_PROXY_MIN_CHARS=0
export COMPRESS_PROXY_ONLY_IF_SMALLER=1
```

Optional alias:

```bash
alias codex="$PWD/scripts/codex-proxy"
alias codex-oneshot="$PWD/scripts/codex-compress"
```

Notes:

- `scripts/codex-compress` only affects the initial message.
- `scripts/codex-proxy` handles follow-up turns too.
- Savings are logged to `/tmp/prompt-compress-proxy.log`, for example:

```text
proxy stats path=responses attempted_blocks=2 rewritten_blocks=1 tokens=128 -> 91 saved=37 ratio=28.9%
```

See also: https://github.com/DevvGwardo/prompt-compress#codex-integration-auto-compress-prompts
