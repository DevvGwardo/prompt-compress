# CLI Reference

Command:

```text
compress [OPTIONS]
```

Options:

- `-i, --input <TEXT>`: direct input text
- `-f, --file <PATH>`: read input from file
- `-a, --aggressiveness <FLOAT>`: compression strength (`0.0` to `1.0`, default `0.5`)
- `-m, --target-model <MODEL>`: tokenizer target model (default `gpt-4`)
- `--onnx`: use ONNX scorer
- `--model-dir <PATH>`: ONNX model directory (or `PROMPT_COMPRESS_MODEL`)
- `-s, --stats`: print token stats
- `--format <text|json>`: output format (default `text`)

Input precedence:

1. `--input`
2. `--file`
3. stdin

See also: https://github.com/DevvGwardo/prompt-compress#cli
