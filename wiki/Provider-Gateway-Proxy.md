# Provider Gateway Proxy

Use `compress-api` as a proxy that compresses user prompt text before forwarding to your model provider.

## Enable

Required:

```bash
export COMPRESS_PROXY_UPSTREAM_BASE_URL="https://api.openai.com/v1"
export COMPRESS_PROXY_UPSTREAM_API_KEY="sk-your-provider-key"
```

Optional:

```bash
export COMPRESS_PROXY_AGGRESSIVENESS=0.4
export COMPRESS_PROXY_TARGET_MODEL="gpt-4"
export COMPRESS_PROXY_MIN_CHARS=80
export COMPRESS_PROXY_ONLY_IF_SMALLER=1
```

For "attempt compression on every non-empty prompt" behavior, set:

```bash
export COMPRESS_PROXY_MIN_CHARS=0
```

Start:

```bash
COMPRESS_API_KEY=sk-local-gateway cargo run --release -p compress-api
```

## Proxy Paths

- `POST /v1/proxy/chat/completions`
- `POST /v1/proxy/responses`

Your client points to `http://localhost:3000` and uses these proxy paths.

## Behavior

- Rewrites user text blocks in request JSON.
- Evaluates token savings for every attempted text block.
- Forwards rewritten request to upstream provider.
- Returns upstream status/body unchanged (streaming included).
- Fails open if rewrite is not possible (request still forwards).
- Logs per-request savings summaries, for example:

```text
proxy stats path=responses attempted_blocks=2 rewritten_blocks=1 tokens=128 -> 91 saved=37 ratio=28.9%
```
