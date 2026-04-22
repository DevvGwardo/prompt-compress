use std::io::{self, Read};

use anyhow::Result;
use clap::Parser;
use compress_core::{find_model, CompressionSettings, Compressor, HeuristicScorer, OnnxScorer};

/// LLM prompt compression tool — reduce token count while preserving meaning.
#[derive(Parser, Debug)]
#[command(name = "compress", version, about)]
struct Args {
    /// Input text to compress (alternative to stdin or -f).
    #[arg(short, long)]
    input: Option<String>,

    /// Read input from a file.
    #[arg(short, long)]
    file: Option<String>,

    /// Compression aggressiveness (0.0 = none, 1.0 = maximum).
    #[arg(short, long, default_value = "0.5")]
    aggressiveness: f32,

    /// Target LLM model for token counting.
    #[arg(short = 'm', long, default_value = "gpt-4")]
    target_model: String,

    /// Use ONNX model for scoring (default: heuristic).
    #[arg(long)]
    onnx: bool,

    /// Path to model directory (contains model.onnx and tokenizer.json).
    #[arg(long, env = "PROMPT_COMPRESS_MODEL")]
    model_dir: Option<String>,

    /// Heuristic scorer mode (Standard: general-purpose, AgentAware: optimized for function calls and instructions).
    #[arg(long, value_enum, default_value = "standard")]
    scorer_mode: ScorerMode,

    /// Show compression statistics.
    #[arg(short, long)]
    stats: bool,

    /// Output format: text or json.
    #[arg(long, default_value = "text")]
    format: OutputFormat,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum ScorerMode {
    Standard,
    AgentAware,
}

impl From<ScorerMode> for compress_core::HeuristicMode {
    fn from(mode: ScorerMode) -> Self {
        match mode {
            ScorerMode::Standard => compress_core::HeuristicMode::Standard,
            ScorerMode::AgentAware => compress_core::HeuristicMode::AgentAware,
        }
    }
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

fn read_input(args: &Args) -> Result<String> {
    if let Some(ref text) = args.input {
        return Ok(text.clone());
    }

    if let Some(ref path) = args.file {
        return Ok(std::fs::read_to_string(path)?);
    }

    // Read from stdin
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    let args = Args::parse();

    let input = read_input(&args)?;
    let input = input.trim();

    if input.is_empty() {
        eprintln!("Error: no input provided. Use -i, -f, or pipe via stdin.");
        std::process::exit(1);
    }

    // Create scorer based on user preference
    let compressor = if args.onnx {
        let model_dir = match args.model_dir {
            Some(dir) => dir,
            None => find_model()?.to_string_lossy().to_string(),
        };
        let scorer = OnnxScorer::load(
            &format!("{}/model.onnx", model_dir),
            &format!("{}/tokenizer.json", model_dir),
        )?;
        Compressor::new(Box::new(scorer), &args.target_model)?
    } else {
        let scorer = HeuristicScorer::new();
        Compressor::new(Box::new(scorer), &args.target_model)?
    };

    let settings = CompressionSettings {
        aggressiveness: args.aggressiveness,
        target_model: args.target_model.clone(),
        scorer_mode: args.scorer_mode.into(),
    };

    let result = compressor.compress(input, &settings)?;

    match args.format {
        OutputFormat::Text => {
            println!("{}", result.output);
            if args.stats {
                eprintln!("---");
                eprintln!("Original tokens:    {}", result.original_input_tokens);
                eprintln!("Compressed tokens:  {}", result.output_tokens);
                eprintln!(
                    "Compression ratio:  {:.1}%",
                    result.compression_ratio * 100.0
                );
                eprintln!(
                    "Tokens saved:       {}",
                    result.original_input_tokens - result.output_tokens
                );
            }
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }

    Ok(())
}
