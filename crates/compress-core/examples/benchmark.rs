use std::env;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use compress_core::{find_model, CompressionSettings, Compressor, HeuristicScorer, OnnxScorer};

#[derive(Clone, Debug)]
struct Args {
    dataset: String,
    samples: usize,
    warmup: usize,
    aggressiveness: f32,
    target_model: String,
    mode: Mode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Heuristic,
    Onnx,
    Both,
}

#[derive(Clone, Debug)]
struct BenchResult {
    mode: &'static str,
    load_ms: f64,
    input_tokens: usize,
    output_tokens: usize,
    ratio: f64,
    saved_pct: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    mean_ms: f64,
    prompts_per_sec: f64,
    input_tokens_per_sec: f64,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        dataset: "training/corpus.txt".to_string(),
        samples: 500,
        warmup: 25,
        aggressiveness: 0.5,
        target_model: "gpt-4".to_string(),
        mode: Mode::Both,
    };

    let mut iter = env::args().skip(1);
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--dataset" => {
                args.dataset = iter
                    .next()
                    .ok_or_else(|| "--dataset requires a value".to_string())?;
            }
            "--samples" => {
                let v = iter
                    .next()
                    .ok_or_else(|| "--samples requires a value".to_string())?;
                args.samples = v
                    .parse::<usize>()
                    .map_err(|_| "invalid --samples value".to_string())?;
            }
            "--warmup" => {
                let v = iter
                    .next()
                    .ok_or_else(|| "--warmup requires a value".to_string())?;
                args.warmup = v
                    .parse::<usize>()
                    .map_err(|_| "invalid --warmup value".to_string())?;
            }
            "--aggressiveness" => {
                let v = iter
                    .next()
                    .ok_or_else(|| "--aggressiveness requires a value".to_string())?;
                args.aggressiveness = v
                    .parse::<f32>()
                    .map_err(|_| "invalid --aggressiveness value".to_string())?;
            }
            "--target-model" => {
                args.target_model = iter
                    .next()
                    .ok_or_else(|| "--target-model requires a value".to_string())?;
            }
            "--mode" => {
                let v = iter
                    .next()
                    .ok_or_else(|| "--mode requires a value".to_string())?;
                args.mode = match v.as_str() {
                    "heuristic" => Mode::Heuristic,
                    "onnx" => Mode::Onnx,
                    "both" => Mode::Both,
                    _ => return Err("invalid --mode value (use heuristic|onnx|both)".to_string()),
                };
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    if !(0.0..=1.0).contains(&args.aggressiveness) {
        return Err("--aggressiveness must be in [0.0, 1.0]".to_string());
    }
    if args.samples == 0 {
        return Err("--samples must be > 0".to_string());
    }

    Ok(args)
}

fn print_help() {
    println!(
        "\
Real benchmark for prompt compression.

Usage:
  cargo run --release -p compress-core --example benchmark -- [options]

Options:
  --dataset <path>         Input dataset file (default: training/corpus.txt)
  --samples <n>            Number of prompts to benchmark (default: 500)
  --warmup <n>             Warmup iterations per mode (default: 25)
  --aggressiveness <f32>   Compression aggressiveness (default: 0.5)
  --target-model <name>    Target model for token counting (default: gpt-4)
  --mode <heuristic|onnx|both>   Scorer mode (default: both)"
    );
}

fn load_prompts(path: &str) -> Result<Vec<String>, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("failed reading {path}: {e}"))?;
    let prompts: Vec<String> = raw
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if prompts.is_empty() {
        return Err(format!("no non-empty prompts found in {path}"));
    }
    Ok(prompts)
}

fn evenly_sample<T: Clone>(items: &[T], n: usize) -> Vec<T> {
    if n >= items.len() {
        return items.to_vec();
    }

    let step = items.len() as f64 / n as f64;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let mut idx = (i as f64 * step).floor() as usize;
        if idx >= items.len() {
            idx = items.len() - 1;
        }
        out.push(items[idx].clone());
    }
    out
}

fn percentile_ms(durations: &[Duration], p: f64) -> f64 {
    if durations.is_empty() {
        return 0.0;
    }
    let mut v: Vec<f64> = durations.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    v.sort_by(|a, b| a.total_cmp(b));
    let idx = ((p / 100.0) * (v.len().saturating_sub(1) as f64)).round() as usize;
    v[idx]
}

fn mean_ms(durations: &[Duration]) -> f64 {
    if durations.is_empty() {
        return 0.0;
    }
    durations
        .iter()
        .map(|d| d.as_secs_f64() * 1000.0)
        .sum::<f64>()
        / durations.len() as f64
}

fn run_with_compressor(
    mode: &'static str,
    compressor: Compressor,
    prompts: &[String],
    warmup: usize,
    settings: &CompressionSettings,
) -> Result<BenchResult, String> {
    let warmup_n = warmup.min(prompts.len());
    for prompt in prompts.iter().take(warmup_n) {
        compressor
            .compress(prompt, settings)
            .map_err(|e| format!("{mode} warmup failed: {e}"))?;
    }

    let mut input_tokens = 0usize;
    let mut output_tokens = 0usize;
    let mut durations = Vec::with_capacity(prompts.len());

    let total_start = Instant::now();
    for prompt in prompts {
        let t0 = Instant::now();
        let result = compressor
            .compress(prompt, settings)
            .map_err(|e| format!("{mode} benchmark failed: {e}"))?;
        durations.push(t0.elapsed());
        input_tokens += result.original_input_tokens;
        output_tokens += result.output_tokens;
    }
    let total = total_start.elapsed().as_secs_f64();

    let ratio = if input_tokens > 0 {
        output_tokens as f64 / input_tokens as f64
    } else {
        1.0
    };
    let saved_pct = (1.0 - ratio) * 100.0;

    Ok(BenchResult {
        mode,
        load_ms: 0.0,
        input_tokens,
        output_tokens,
        ratio,
        saved_pct,
        p50_ms: percentile_ms(&durations, 50.0),
        p95_ms: percentile_ms(&durations, 95.0),
        p99_ms: percentile_ms(&durations, 99.0),
        mean_ms: mean_ms(&durations),
        prompts_per_sec: if total > 0.0 {
            prompts.len() as f64 / total
        } else {
            0.0
        },
        input_tokens_per_sec: if total > 0.0 {
            input_tokens as f64 / total
        } else {
            0.0
        },
    })
}

fn run_heuristic(
    prompts: &[String],
    warmup: usize,
    settings: &CompressionSettings,
) -> Result<BenchResult, String> {
    let load_start = Instant::now();
    let compressor = Compressor::new(Box::new(HeuristicScorer::new()), &settings.target_model)
        .map_err(|e| format!("failed to init HeuristicScorer: {e}"))?;
    let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;

    let mut result = run_with_compressor("Heuristic", compressor, prompts, warmup, settings)?;
    result.load_ms = load_ms;
    Ok(result)
}

fn run_onnx(
    prompts: &[String],
    warmup: usize,
    settings: &CompressionSettings,
) -> Result<BenchResult, String> {
    let model_dir = find_model().map_err(|e| format!("failed finding model directory: {e}"))?;
    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    let load_start = Instant::now();
    let scorer = OnnxScorer::load(path_to_str(&model_path)?, path_to_str(&tokenizer_path)?)
        .map_err(|e| format!("failed to load ONNX scorer: {e}"))?;
    let compressor = Compressor::new(Box::new(scorer), &settings.target_model)
        .map_err(|e| format!("failed to init ONNX compressor: {e}"))?;
    let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;

    let mut result = run_with_compressor("ONNX", compressor, prompts, warmup, settings)?;
    result.load_ms = load_ms;
    Ok(result)
}

fn path_to_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn print_table(results: &[BenchResult], args: &Args) {
    println!("# prompt-compress benchmark");
    println!("dataset: {}", args.dataset);
    println!("samples: {}", args.samples);
    println!("warmup: {}", args.warmup);
    println!("aggressiveness: {}", args.aggressiveness);
    println!("target_model: {}", args.target_model);
    println!();
    println!(
        "| Mode | Load (ms) | p50 (ms) | p95 (ms) | p99 (ms) | Mean (ms) | Prompts/s | Input tok/s | Tokens In -> Out | Ratio | Saved |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    for r in results {
        println!(
            "| {} | {:.1} | {:.2} | {:.2} | {:.2} | {:.2} | {:.1} | {:.0} | {} -> {} | {:.3} | {:.1}% |",
            r.mode,
            r.load_ms,
            r.p50_ms,
            r.p95_ms,
            r.p99_ms,
            r.mean_ms,
            r.prompts_per_sec,
            r.input_tokens_per_sec,
            r.input_tokens,
            r.output_tokens,
            r.ratio,
            r.saved_pct
        );
    }
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    let prompts = load_prompts(&args.dataset)?;
    let sampled = evenly_sample(&prompts, args.samples);

    let settings = CompressionSettings {
        aggressiveness: args.aggressiveness,
        target_model: args.target_model.clone(),
        ..Default::default()
    };

    let mut results = Vec::new();
    if args.mode == Mode::Heuristic || args.mode == Mode::Both {
        results.push(run_heuristic(&sampled, args.warmup, &settings)?);
    }
    if args.mode == Mode::Onnx || args.mode == Mode::Both {
        results.push(run_onnx(&sampled, args.warmup, &settings)?);
    }

    print_table(&results, &args);
    Ok(())
}
