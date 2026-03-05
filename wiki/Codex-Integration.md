# Codex Integration

Use the included wrapper script so prompts are compressed before `codex` runs.

Script path:

- `scripts/codex-compress`

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

Recommended environment:

```bash
export PROMPT_COMPRESS_AGGRESSIVENESS=0.4
export PROMPT_COMPRESS_USE_ONNX=0
export PROMPT_COMPRESS_MODEL="$PWD/models"
export PROMPT_COMPRESS_BIN="$PWD/target/release/compress"
export PROMPT_COMPRESS_INTERACTIVE_FIRST_PROMPT=1
```

Optional alias:

```bash
alias codex="$PWD/scripts/codex-compress"
```

Notes:

- Interactive `codex` sessions get initial-message compression through this wrapper.
- In-session follow-up messages are handled by native `codex` UI flow.

See also: https://github.com/DevvGwardo/prompt-compress#codex-integration-auto-compress-prompts
