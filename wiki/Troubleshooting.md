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

- prefer `scripts/codex-proxy` over `scripts/codex-compress`
- the launcher should route Codex via `chatgpt_base_url`, not `OPENAI_BASE_URL`
- upstream should be `https://chatgpt.com/backend-api`

## Compression not reducing tokens

- lower `aggressiveness` may preserve too much text
- with `onlyIfSmaller=true`, plugin intentionally skips non-improving rewrites
- try heuristic vs ONNX and compare

## Codex wrapper seems to save no tokens

Check:

- your shell alias points to `scripts/codex-compress`
- `PROMPT_COMPRESS_BIN` points to a valid `compress` binary
- for plain `codex` launches, provide the initial prompt in the wrapper prompt
- set `PROMPT_COMPRESS_INTERACTIVE_FIRST_PROMPT=1` to enable the initial prompt flow

Note:

- interactive Codex follow-up turns are currently not rewritten by this wrapper

## Codex proxy stats are missing

Check:

- your shell alias points to `scripts/codex-proxy`
- `compress-api` was restarted after updates (`pkill -f compress-api || true`)
- watch `/tmp/prompt-compress-proxy.log`
- prompts may be evaluated but not rewritten if `onlyIfSmaller=true` and token count does not improve

## Need API mode instead of CLI

Start API server:

```bash
cargo run --release -p compress-api
```

Docs:
- https://github.com/DevvGwardo/prompt-compress#api
