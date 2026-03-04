use axum::{extract::State, http::StatusCode, Json};

use compress_core::CompressionSettings;

use crate::dto::{CompressRequest, CompressResponse, ErrorDetail, ErrorResponse};
use crate::state::AppState;

/// POST /v1/compress
pub async fn compress(
    State(state): State<AppState>,
    Json(req): Json<CompressRequest>,
) -> Result<Json<CompressResponse>, (StatusCode, Json<ErrorResponse>)> {
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
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: ErrorDetail {
                    message: e.to_string(),
                    r#type: "invalid_request_error".to_string(),
                },
            }),
        )),
    }
}

/// GET /health
pub async fn health() -> &'static str {
    "ok"
}
