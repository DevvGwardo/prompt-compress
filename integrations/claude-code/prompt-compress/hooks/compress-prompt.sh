#!/usr/bin/env bash
# UserPromptSubmit hook: compress user prompts before they reach Claude.
#
# Reads the user_prompt from stdin JSON, runs it through the compress binary,
# and outputs a systemMessage with the compressed version so Claude sees
# fewer tokens while preserving meaning.
#
# Env config:
#   PROMPT_COMPRESS_BIN            Path to the compress binary
#   PROMPT_COMPRESS_AGGRESSIVENESS Compression level 0.0-1.0 (default: 0.4)
#   PROMPT_COMPRESS_MIN_CHARS      Skip compression for short prompts (default: 80)
#   PROMPT_COMPRESS_TARGET_MODEL   Tokenizer target model (default: gpt-4)
#   PROMPT_COMPRESS_USE_ONNX       Set to 1 to use ONNX scorer
#   PROMPT_COMPRESS_MODEL          ONNX model directory
#   PROMPT_COMPRESS_LOG            Log file path (default: /tmp/prompt-compress-hook.log)
set -euo pipefail

# Fail open if jq is not available — the hook cannot parse input without it
if ! command -v jq >/dev/null 2>&1; then
  exit 0
fi

AGGRESSIVENESS="${PROMPT_COMPRESS_AGGRESSIVENESS:-0.4}"
TARGET_MODEL="${PROMPT_COMPRESS_TARGET_MODEL:-gpt-4}"
MIN_CHARS="${PROMPT_COMPRESS_MIN_CHARS:-80}"
USE_ONNX="${PROMPT_COMPRESS_USE_ONNX:-0}"
MODEL_DIR="${PROMPT_COMPRESS_MODEL:-}"
LOG_FILE="${PROMPT_COMPRESS_LOG:-/tmp/prompt-compress-hook.log}"
# Approximate token overhead of the systemMessage wrapper text.
# The wrapper replaces the original prompt (suppressUserPrompt), so the net
# cost is (compressed_tokens + wrapper) vs original_tokens.
WRAPPER_OVERHEAD=45

# Read hook input from stdin
input="$(cat)"
user_prompt="$(printf '%s' "$input" | jq -r '.user_prompt // empty')"

# If no prompt or too short, pass through silently
if [[ -z "$user_prompt" ]] || [[ ${#user_prompt} -lt $MIN_CHARS ]]; then
  exit 0
fi

# Resolve compress binary
resolve_compress_bin() {
  if [[ -n "${PROMPT_COMPRESS_BIN:-}" && -x "${PROMPT_COMPRESS_BIN}" ]]; then
    echo "${PROMPT_COMPRESS_BIN}"
    return 0
  fi

  # Check common locations relative to plugin root
  local plugin_root="${CLAUDE_PLUGIN_ROOT:-}"
  if [[ -n "$plugin_root" ]]; then
    # Navigate up from integrations/claude-code/prompt-compress to project root
    local project_root
    project_root="$(cd "$plugin_root/../../.." 2>/dev/null && pwd)" || true
    if [[ -n "$project_root" && -x "$project_root/target/release/compress" ]]; then
      echo "$project_root/target/release/compress"
      return 0
    fi
  fi

  # Check project dir
  local project_dir="${CLAUDE_PROJECT_DIR:-}"
  if [[ -n "$project_dir" && -x "$project_dir/target/release/compress" ]]; then
    echo "$project_dir/target/release/compress"
    return 0
  fi

  # Check PATH
  local candidate
  candidate="$(command -v compress 2>/dev/null || true)"
  if [[ -n "$candidate" ]] && "$candidate" --help 2>&1 | grep -q "LLM prompt compression tool"; then
    echo "$candidate"
    return 0
  fi

  return 1
}

COMPRESS_BIN="$(resolve_compress_bin || true)"
if [[ -z "$COMPRESS_BIN" ]]; then
  # No binary found — fail open, let prompt through unchanged
  exit 0
fi

# Build compress command
compress_cmd=(
  "$COMPRESS_BIN"
  -i "$user_prompt"
  -a "$AGGRESSIVENESS"
  -m "$TARGET_MODEL"
  --format json
)

if [[ "$USE_ONNX" == "1" ]]; then
  compress_cmd+=(--onnx)
fi

if [[ -n "$MODEL_DIR" ]]; then
  compress_cmd+=(--model-dir "$MODEL_DIR")
fi

# Run compression
compress_json="$("${compress_cmd[@]}" 2>/dev/null || true)"

if [[ -z "$compress_json" ]]; then
  # Compression failed — fail open
  exit 0
fi

compressed="$(jq -r '.output // empty' <<< "$compress_json" 2>/dev/null || true)"
original_tokens="$(jq -r '.original_input_tokens // 0' <<< "$compress_json" 2>/dev/null || true)"
output_tokens="$(jq -r '.output_tokens // 0' <<< "$compress_json" 2>/dev/null || true)"

# Only use compressed version if it's actually shorter and non-empty.
# Account for the wrapper text overhead so we don't increase total tokens.
if [[ -z "$compressed" ]] || [[ -z "${compressed//[[:space:]]/}" ]]; then
  exit 0
fi

total_with_wrapper=$(( output_tokens + WRAPPER_OVERHEAD ))
if [[ "$total_with_wrapper" -ge "$original_tokens" ]]; then
  # Compressed + wrapper is not smaller than the original — pass through unchanged
  exit 0
fi

saved=$(( original_tokens - total_with_wrapper ))

# Log stats
{
  stamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null || true)"
  printf '%s hook compress original=%s compressed=%s saved=%s\n' \
    "${stamp:-unknown}" "$original_tokens" "$output_tokens" "$saved"
} >> "$LOG_FILE" 2>/dev/null || true

# Output: inject compressed prompt as a systemMessage so Claude processes fewer tokens.
# The original user_prompt is what the user typed; the systemMessage tells Claude
# to use the compressed version instead.
jq -n \
  --arg compressed "$compressed" \
  --arg original_tokens "$original_tokens" \
  --arg output_tokens "$output_tokens" \
  --arg saved "$saved" \
  '{
    "hookSpecificOutput": {
      "suppressUserPrompt": true
    },
    "systemMessage": ("The user prompt has been compressed to save tokens. Use the following compressed prompt as the user request (do NOT ask the user to repeat themselves):\n\n" + $compressed + "\n\n[prompt-compress: " + $original_tokens + " -> " + $output_tokens + " tokens, saved " + $saved + "]")
  }'
