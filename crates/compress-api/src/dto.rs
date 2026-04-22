use serde::{Deserialize, Serialize};

/// Request body for POST /v1/compress.
#[derive(Debug, Deserialize)]
pub struct CompressRequest {
    /// Scoring model identifier. Supported: "scorer-v0.1", "heuristic-v0.1", "heuristic-agent-v0.1".
    #[serde(default = "default_model")]
    pub model: String,

    /// The text to compress.
    pub input: String,

    /// Compression settings.
    #[serde(default)]
    pub compression_settings: CompressSettingsDto,
}

/// Request body for POST /v1/compress/preset/:name.
#[derive(Debug, Deserialize)]
pub struct CompressPresetRequest {
    /// The text to compress.
    pub input: String,

    /// Target LLM model for token counting.
    #[serde(default = "default_target_model")]
    pub target_model: String,
}

fn default_model() -> String {
    "scorer-v0.1".to_string()
}

#[derive(Debug, Default, Deserialize)]
pub struct CompressSettingsDto {
    #[serde(default = "default_aggressiveness")]
    pub aggressiveness: f32,

    #[serde(default = "default_target_model")]
    pub target_model: String,
}

fn default_aggressiveness() -> f32 {
    0.5
}

fn default_target_model() -> String {
    "gpt-4".to_string()
}

/// Response body for POST /v1/compress.
#[derive(Debug, Serialize)]
pub struct CompressResponse {
    pub output: String,
    pub output_tokens: usize,
    pub original_input_tokens: usize,
    pub compression_ratio: f64,
}

/// Response body for POST /v1/compress/preset/:name.
#[derive(Debug, Serialize)]
pub struct CompressPresetResponse {
    pub preset: String,
    pub output: String,
    pub output_tokens: usize,
    pub original_input_tokens: usize,
    pub compression_ratio: f64,
}

/// Error response body.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub message: String,
    pub r#type: String,
}
