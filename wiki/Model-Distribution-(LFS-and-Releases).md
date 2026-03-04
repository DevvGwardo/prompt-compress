# Model Distribution (LFS and Releases)

When ONNX mode is enabled, `prompt-compress` looks for model artifacts in this order:

1. `PROMPT_COMPRESS_MODEL`
2. `./models`
3. `models/scorer-v0.1`
4. `../models`
5. `../../models`

Required files:

- `model.onnx`
- `tokenizer.json`

Distribution options:

1. Git LFS for repository-managed binaries
2. GitHub Release assets for distribution-only binaries

Example release:

- https://github.com/DevvGwardo/prompt-compress/releases/tag/model-v0.1.0
