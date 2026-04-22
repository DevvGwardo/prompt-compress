use serde::{Deserialize, Serialize};

use crate::scorer::HeuristicMode;

/// Settings controlling compression behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionSettings {
    /// How aggressively to compress (0.0 = keep everything, 1.0 = remove as much as possible).
    #[serde(default = "default_aggressiveness")]
    pub aggressiveness: f32,

    /// Target LLM model for token counting (e.g. "gpt-4", "claude").
    #[serde(default = "default_target_model")]
    pub target_model: String,

    /// Scoring mode to use for token importance.
    #[serde(default = "default_scorer_mode")]
    pub scorer_mode: HeuristicMode,
}

fn default_aggressiveness() -> f32 {
    0.5
}

fn default_target_model() -> String {
    "gpt-4".to_string()
}

fn default_scorer_mode() -> HeuristicMode {
    HeuristicMode::Standard
}

impl Default for CompressionSettings {
    fn default() -> Self {
        Self {
            aggressiveness: default_aggressiveness(),
            target_model: default_target_model(),
            scorer_mode: default_scorer_mode(),
        }
    }
}

/// Result of a compression operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    /// The compressed text.
    pub output: String,

    /// Number of LLM tokens in the compressed output.
    pub output_tokens: usize,

    /// Number of LLM tokens in the original input.
    pub original_input_tokens: usize,

    /// Compression ratio (output_tokens / original_input_tokens).
    pub compression_ratio: f64,
}
