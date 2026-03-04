import { spawn } from "node:child_process";
import type { OpenClawPluginApi } from "openclaw/plugin-sdk";

type PromptCompressPluginConfig = {
  command?: string;
  args?: string[];
  aggressiveness?: number;
  targetModel?: string;
  useOnnx?: boolean;
  modelDir?: string;
  minChars?: number;
  timeoutMs?: number;
  onlyIfSmaller?: boolean;
  env?: Record<string, string>;
};

type ResolvedConfig = {
  command: string;
  args: string[];
  aggressiveness: number;
  targetModel: string;
  useOnnx: boolean;
  modelDir?: string;
  minChars: number;
  timeoutMs: number;
  onlyIfSmaller: boolean;
  env: Record<string, string>;
};

type CompressCliResult = {
  output: string;
  outputTokens?: number;
  originalTokens?: number;
};

function parseBool(value: unknown): boolean | undefined {
  if (typeof value === "boolean") {
    return value;
  }
  if (typeof value !== "string") {
    return undefined;
  }
  const normalized = value.trim().toLowerCase();
  if (["1", "true", "yes", "on"].includes(normalized)) {
    return true;
  }
  if (["0", "false", "no", "off"].includes(normalized)) {
    return false;
  }
  return undefined;
}

function parseNumber(value: unknown): number | undefined {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string") {
    const n = Number(value);
    if (Number.isFinite(n)) {
      return n;
    }
  }
  return undefined;
}

function normalizeCommand(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed || undefined;
}

function normalizeArgs(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .filter((item): item is string => typeof item === "string")
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
}

function normalizeEnv(value: unknown): Record<string, string> {
  if (!value || typeof value !== "object") {
    return {};
  }
  const record = value as Record<string, unknown>;
  const out: Record<string, string> = {};
  for (const [key, val] of Object.entries(record)) {
    if (typeof val === "string") {
      out[key] = val;
    }
  }
  return out;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function resolveConfig(input: unknown): ResolvedConfig {
  const cfg = ((input ?? {}) as PromptCompressPluginConfig) ?? {};

  const envAgg = parseNumber(process.env.PROMPT_COMPRESS_AGGRESSIVENESS);
  const envUseOnnx = parseBool(process.env.PROMPT_COMPRESS_USE_ONNX);

  const command =
    normalizeCommand(cfg.command) ?? normalizeCommand(process.env.PROMPT_COMPRESS_BIN) ?? "compress";

  const args = normalizeArgs(cfg.args);
  const aggressiveness = clamp(parseNumber(cfg.aggressiveness) ?? envAgg ?? 0.4, 0, 1);
  const targetModel =
    normalizeCommand(cfg.targetModel) ?? normalizeCommand(process.env.PROMPT_COMPRESS_TARGET_MODEL) ?? "gpt-4";
  const useOnnx = parseBool(cfg.useOnnx) ?? envUseOnnx ?? false;
  const modelDir = normalizeCommand(cfg.modelDir) ?? normalizeCommand(process.env.PROMPT_COMPRESS_MODEL);

  const minChars = Math.max(1, Math.floor(parseNumber(cfg.minChars) ?? 80));
  const timeoutMs = Math.max(100, Math.floor(parseNumber(cfg.timeoutMs) ?? 2000));
  const onlyIfSmaller = parseBool(cfg.onlyIfSmaller) ?? true;

  return {
    command,
    args,
    aggressiveness,
    targetModel,
    useOnnx,
    modelDir,
    minChars,
    timeoutMs,
    onlyIfSmaller,
    env: normalizeEnv(cfg.env),
  };
}

async function runPromptCompress(params: {
  prompt: string;
  config: ResolvedConfig;
}): Promise<CompressCliResult> {
  const { prompt, config } = params;

  return await new Promise<CompressCliResult>((resolve, reject) => {
    const cliArgs = [
      ...config.args,
      "--format",
      "json",
      "--aggressiveness",
      String(config.aggressiveness),
      "--target-model",
      config.targetModel,
    ];

    if (config.useOnnx) {
      cliArgs.push("--onnx");
    }
    if (config.modelDir) {
      cliArgs.push("--model-dir", config.modelDir);
    }

    const child = spawn(config.command, cliArgs, {
      env: {
        ...process.env,
        ...config.env,
      },
      stdio: ["pipe", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";
    let timedOut = false;

    const timer = setTimeout(() => {
      timedOut = true;
      child.kill("SIGKILL");
    }, config.timeoutMs);

    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString();
    });

    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
    });

    child.on("error", (err) => {
      clearTimeout(timer);
      reject(new Error(`prompt-compress spawn failed: ${String(err)}`));
    });

    child.on("close", (code) => {
      clearTimeout(timer);
      if (timedOut) {
        reject(new Error(`prompt-compress timed out after ${config.timeoutMs}ms`));
        return;
      }
      if (code !== 0) {
        const reason = stderr.trim() || `exit code ${code}`;
        reject(new Error(`prompt-compress failed: ${reason}`));
        return;
      }

      let parsed: unknown;
      try {
        parsed = JSON.parse(stdout);
      } catch (err) {
        reject(new Error(`prompt-compress returned invalid JSON: ${String(err)}`));
        return;
      }

      if (!parsed || typeof parsed !== "object") {
        reject(new Error("prompt-compress returned invalid payload"));
        return;
      }

      const payload = parsed as Record<string, unknown>;
      const output = typeof payload.output === "string" ? payload.output : "";
      const outputTokens = parseNumber(payload.output_tokens);
      const originalTokens = parseNumber(payload.original_input_tokens);

      if (!output.trim()) {
        reject(new Error("prompt-compress returned an empty output prompt"));
        return;
      }

      resolve({ output, outputTokens, originalTokens });
    });

    child.stdin.end(prompt);
  });
}

const promptCompressPlugin = {
  id: "prompt-compress",
  name: "Prompt Compress",
  description: "Compresses prompts before model execution using the prompt-compress CLI.",
  configSchema: {
    type: "object",
    additionalProperties: false,
    properties: {
      command: { type: "string" },
      args: { type: "array", items: { type: "string" } },
      aggressiveness: { type: "number", minimum: 0, maximum: 1 },
      targetModel: { type: "string" },
      useOnnx: { type: "boolean" },
      modelDir: { type: "string" },
      minChars: { type: "number", minimum: 1 },
      timeoutMs: { type: "number", minimum: 100 },
      onlyIfSmaller: { type: "boolean" },
      env: {
        type: "object",
        additionalProperties: {
          type: "string",
        },
      },
    },
  },
  register(api: OpenClawPluginApi) {
    const cfg = resolveConfig(api.pluginConfig);

    api.on("before_prompt_build", async (event) => {
      const prompt = typeof event.prompt === "string" ? event.prompt : "";
      if (!prompt || prompt.length < cfg.minChars) {
        return;
      }

      try {
        const compressed = await runPromptCompress({ prompt, config: cfg });

        if (cfg.onlyIfSmaller) {
          const outputTokens = compressed.outputTokens;
          const originalTokens = compressed.originalTokens;
          if (
            typeof outputTokens === "number" &&
            typeof originalTokens === "number" &&
            outputTokens >= originalTokens
          ) {
            return;
          }
        }

        if (compressed.output === prompt) {
          return;
        }

        api.logger.info?.(
          `prompt-compress: prompt override applied (${prompt.length} -> ${compressed.output.length} chars)`,
        );

        return {
          promptOverride: compressed.output,
        };
      } catch (err) {
        api.logger.warn?.(`prompt-compress: failed to compress prompt (${String(err)})`);
        return;
      }
    });
  },
};

export default promptCompressPlugin;
