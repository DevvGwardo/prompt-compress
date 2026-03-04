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

## Compression not reducing tokens

- lower `aggressiveness` may preserve too much text
- with `onlyIfSmaller=true`, plugin intentionally skips non-improving rewrites
- try heuristic vs ONNX and compare

## Need API mode instead of CLI

Start API server:

```bash
cargo run --release -p compress-api
```

Docs:
- https://github.com/DevvGwardo/prompt-compress#api
