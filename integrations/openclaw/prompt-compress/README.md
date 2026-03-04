# OpenClaw Prompt-Compress Plugin

This integration package lets OpenClaw call `prompt-compress` before model execution.

It uses `before_prompt_build` and returns `promptOverride`, so the compressed prompt is what reaches the model.

## Requirements

- OpenClaw build that supports `before_prompt_build.promptOverride`
- `prompt-compress` binary available on PATH, or configured with `command`

## Install from this repo

```bash
openclaw plugins install /absolute/path/to/prompt-compress/integrations/openclaw/prompt-compress
openclaw plugins enable prompt-compress
```

## Recommended config

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

Restart OpenClaw Gateway after enabling or changing plugin config.
