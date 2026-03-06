# Troubleshooting

## `Error: no input provided`

Provide one of:

- `--input`
- `--file`
- stdin piping

## ONNX mode fails to load

Check:

- `--model-dir` path exists
- `model.onnx` and `tokenizer.json` are present
- file permissions allow read access

## OpenClaw plugin enabled but no compression

Check:

- plugin is enabled (`openclaw plugins list`)
- plugin config `command` points to a valid `compress` binary
- OpenClaw build supports `before_prompt_build.promptOverride`
- gateway restarted after config changes

## Proxy mode returns upstream errors

Check:

- `COMPRESS_PROXY_UPSTREAM_BASE_URL` is set and reachable
- `COMPRESS_PROXY_UPSTREAM_API_KEY` is valid (or your upstream auth is passed through)
- client calls `/v1/proxy/chat/completions` or `/v1/proxy/responses`
- local gateway auth header matches `COMPRESS_API_KEY`

For ChatGPT-plan Codex:

- the HTTP proxy path is not reliable for native interactive turns
- use `scripts/codex-chat-compress` for deterministic per-turn compression
- use `scripts/codex-proxy` only as a compatibility shim to the chat wrapper

## Compression not reducing tokens

- lower `aggressiveness` may preserve too much text
- with `onlyIfSmaller=true`, plugin intentionally skips non-improving rewrites
- try heuristic vs ONNX and compare

## Codex wrapper seems to save no tokens

Check:

- your shell alias points to `scripts/codex-chat-compress`
- `PROMPT_COMPRESS_BIN` points to a valid `compress` binary
- finish the prompt with `/send`
- look for wrapper output like `[prompt-compress] tokens 1280 -> 914 saved=366 (28.6%) rewritten`

Note:

- the wrapper only sends the compressed text when it is smaller than the original
- use `/status` to verify a Codex thread is being tracked
- use `/new` to force the next prompt into a fresh thread

## Codex savings log is missing

Check:

- your shell alias points to `scripts/codex-chat-compress`
- you launched `codex-native` instead of `codex`
- at least one prompt has been sent with `/send`
- watch `/tmp/prompt-compress-codex-chat.log`

## Need API mode instead of CLI

Start API server:

```bash
cargo run --release -p compress-api
```

Docs:
- https://github.com/DevvGwardo/prompt-compress#api
