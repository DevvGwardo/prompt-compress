# OpenClaw Integration

OpenClaw plugin package in this repo:

- `integrations/openclaw/prompt-compress`

Install and enable:

```bash
openclaw plugins install /absolute/path/to/prompt-compress/integrations/openclaw/prompt-compress
openclaw plugins enable prompt-compress
```

Recommended config:

```json
{
  "plugins": {
    "entries": {
      "prompt-compress": {
        "enabled": true,
        "config": {
          "command": "/absolute/path/to/prompt-compress/target/release/compress",
          "aggressiveness": 0.4,
          "targetModel": "gpt-4",
          "useOnnx": false,
          "modelDir": "/absolute/path/to/prompt-compress/models",
          "minChars": 80,
          "timeoutMs": 2000,
          "onlyIfSmaller": true
        }
      }
    }
  }
}
```

Notes:

- Requires OpenClaw build support for `before_prompt_build.promptOverride`.
- Plugin fails open: if compression errors, original prompt is used.

Source docs:
- https://github.com/DevvGwardo/prompt-compress/tree/master/integrations/openclaw/prompt-compress
