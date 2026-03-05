use axum::{extract::State, http::StatusCode, Json};

use compress_core::CompressionSettings;

use crate::dto::{CompressRequest, CompressResponse, ErrorDetail, ErrorResponse};
use crate::state::AppState;

fn bad_request(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: ErrorDetail {
                message: message.into(),
                r#type: "invalid_request_error".to_string(),
            },
        }),
    )
}

/// POST /v1/compress
pub async fn compress(
    State(state): State<AppState>,
    Json(req): Json<CompressRequest>,
) -> Result<Json<CompressResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Keep model explicit so callers don't assume arbitrary model IDs are accepted.
    if req.model != "scorer-v0.1" && req.model != "heuristic-v0.1" {
        return Err(bad_request(format!(
            "unsupported model '{}'; supported values: scorer-v0.1, heuristic-v0.1",
            req.model
        )));
    }

    let settings = CompressionSettings {
        aggressiveness: req.compression_settings.aggressiveness,
        target_model: req.compression_settings.target_model,
    };

    match state.compressor.compress(&req.input, &settings) {
        Ok(result) => Ok(Json(CompressResponse {
            output: result.output,
            output_tokens: result.output_tokens,
            original_input_tokens: result.original_input_tokens,
            compression_ratio: result.compression_ratio,
        })),
        Err(e) => Err(bad_request(e.to_string())),
    }
}

/// GET /health
pub async fn health() -> &'static str {
    "ok"
}
