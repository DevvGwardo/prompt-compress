use axum::{middleware as axum_mw, response::IntoResponse, routing, Router};
use axum_test::TestServer;
use serde_json::json;
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::decompression::RequestDecompressionLayer;

use compress_core::{Compressor, HeuristicMode, HeuristicScorer};

#[derive(Clone)]
struct AppState {
    compressor: Arc<Compressor>,
    api_key: Option<String>,
}

async fn auth(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    let Some(ref expected_key) = state.api_key else {
        return Ok(next.run(req).await);
    };
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());
    match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = &header[7..];
            if token == expected_key {
                Ok(next.run(req).await)
            } else {
                Err(axum::http::StatusCode::UNAUTHORIZED)
            }
        }
        _ => Err(axum::http::StatusCode::UNAUTHORIZED),
    }
}

async fn compress_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::Json(req): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let model = req
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("scorer-v0.1");
    if model != "scorer-v0.1" && model != "heuristic-v0.1" {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(
                json!({"error": {"message": "unsupported model", "type": "invalid_request_error"}}),
            ),
        )
            .into_response();
    }

    let input = req["input"].as_str().unwrap_or_default();
    let aggressiveness = req
        .get("compression_settings")
        .and_then(|s| s.get("aggressiveness"))
        .and_then(|a| a.as_f64())
        .unwrap_or(0.5) as f32;

    let settings = compress_core::CompressionSettings {
        aggressiveness,
        target_model: "gpt-4".to_string(),
        scorer_mode: HeuristicMode::Standard,
    };

    match state.compressor.compress(input, &settings) {
        Ok(result) => axum::Json(json!({
            "output": result.output,
            "output_tokens": result.output_tokens,
            "original_input_tokens": result.original_input_tokens,
            "compression_ratio": result.compression_ratio,
        }))
        .into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(
                json!({"error": {"message": e.to_string(), "type": "invalid_request_error"}}),
            ),
        )
            .into_response(),
    }
}

fn build_app(api_key: Option<String>) -> Router {
    let scorer = HeuristicScorer::new();
    let compressor = Arc::new(Compressor::new(Box::new(scorer), "gpt-4").unwrap());
    let state = AppState {
        compressor,
        api_key,
    };

    let api_routes = Router::new()
        .route("/v1/compress", routing::post(compress_handler))
        .route_layer(axum_mw::from_fn_with_state(state.clone(), auth));

    Router::new()
        .merge(api_routes)
        .route("/health", routing::get(|| async { "ok" }))
        .layer(CompressionLayer::new())
        .layer(RequestDecompressionLayer::new())
        .with_state(state)
}

// ─── Response schema validation ──────────────────────────────────────

#[tokio::test]
async fn response_contains_all_required_fields() {
    let server = TestServer::new(build_app(None));
    let response = server
        .post("/v1/compress")
        .json(&json!({
            "input": "the quick brown fox",
            "compression_settings": { "aggressiveness": 0.5 }
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();

    assert!(body.get("output").is_some(), "missing 'output' field");
    assert!(
        body.get("output_tokens").is_some(),
        "missing 'output_tokens'"
    );
    assert!(
        body.get("original_input_tokens").is_some(),
        "missing 'original_input_tokens'"
    );
    assert!(
        body.get("compression_ratio").is_some(),
        "missing 'compression_ratio'"
    );
}

#[tokio::test]
async fn compression_ratio_is_between_zero_and_one() {
    let server = TestServer::new(build_app(None));
    let response = server
        .post("/v1/compress")
        .json(&json!({
            "input": "the quick brown fox jumps over the lazy dog",
            "compression_settings": { "aggressiveness": 0.5 }
        }))
        .await;

    let body: serde_json::Value = response.json();
    let ratio = body["compression_ratio"].as_f64().unwrap();
    assert!((0.0..=1.0).contains(&ratio), "ratio {ratio} out of bounds");
}

#[tokio::test]
async fn output_tokens_lte_original() {
    let server = TestServer::new(build_app(None));
    let response = server
        .post("/v1/compress")
        .json(&json!({
            "input": "the quick brown fox jumps over the lazy dog",
            "compression_settings": { "aggressiveness": 0.5 }
        }))
        .await;

    let body: serde_json::Value = response.json();
    let output_tokens = body["output_tokens"].as_u64().unwrap();
    let original_tokens = body["original_input_tokens"].as_u64().unwrap();
    assert!(output_tokens <= original_tokens);
}

// ─── Aggressiveness via API ──────────────────────────────────────────

#[tokio::test]
async fn aggressiveness_zero_returns_unchanged() {
    let server = TestServer::new(build_app(None));
    let input = "hello world";
    let response = server
        .post("/v1/compress")
        .json(&json!({
            "input": input,
            "compression_settings": { "aggressiveness": 0.0 }
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["output"].as_str().unwrap(), input);
    assert_eq!(body["compression_ratio"].as_f64().unwrap(), 1.0);
}

#[tokio::test]
async fn default_aggressiveness_when_omitted() {
    let server = TestServer::new(build_app(None));
    let response = server
        .post("/v1/compress")
        .json(&json!({ "input": "the quick brown fox jumps over the lazy dog" }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    // Default aggressiveness = 0.5, should compress somewhat
    let ratio = body["compression_ratio"].as_f64().unwrap();
    assert!(ratio < 1.0, "default aggressiveness should compress");
}

#[tokio::test]
async fn increasing_aggressiveness_reduces_tokens() {
    let server = TestServer::new(build_app(None));
    let input =
        "the quick brown fox jumps over the lazy dog and it was a very good day for everyone";

    let low = server
        .post("/v1/compress")
        .json(&json!({ "input": input, "compression_settings": { "aggressiveness": 0.3 } }))
        .await;
    let high = server
        .post("/v1/compress")
        .json(&json!({ "input": input, "compression_settings": { "aggressiveness": 0.8 } }))
        .await;

    let low_tokens = low.json::<serde_json::Value>()["output_tokens"]
        .as_u64()
        .unwrap();
    let high_tokens = high.json::<serde_json::Value>()["output_tokens"]
        .as_u64()
        .unwrap();

    assert!(low_tokens >= high_tokens);
}

// ─── Safe tags via API ───────────────────────────────────────────────

#[tokio::test]
async fn safe_tags_survive_api_compression() {
    let server = TestServer::new(build_app(None));
    let response = server
        .post("/v1/compress")
        .json(&json!({
            "input": "remove filler but <ttc_safe>critical data</ttc_safe> stays",
            "compression_settings": { "aggressiveness": 0.9 }
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let output = body["output"].as_str().unwrap();
    assert!(output.contains("critical"));
    assert!(output.contains("data"));
}

// ─── Error responses ─────────────────────────────────────────────────

#[tokio::test]
async fn empty_input_returns_400() {
    let server = TestServer::new(build_app(None));
    let response = server
        .post("/v1/compress")
        .json(&json!({ "input": "" }))
        .await;

    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert!(body["error"]["message"].as_str().unwrap().contains("empty"));
    assert_eq!(
        body["error"]["type"].as_str().unwrap(),
        "invalid_request_error"
    );
}

#[tokio::test]
async fn whitespace_only_input_returns_400() {
    let server = TestServer::new(build_app(None));
    let response = server
        .post("/v1/compress")
        .json(&json!({ "input": "   " }))
        .await;

    response.assert_status_bad_request();
}

#[tokio::test]
async fn invalid_json_returns_error() {
    let server = TestServer::new(build_app(None));
    let response = server
        .post("/v1/compress")
        .content_type("application/json")
        .bytes(b"not json".to_vec().into())
        .await;

    // axum returns 422 for unprocessable JSON
    let status = response.status_code();
    assert!(
        status == http::StatusCode::BAD_REQUEST || status == http::StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400 or 422, got {status}"
    );
}

#[tokio::test]
async fn unsupported_model_returns_400() {
    let server = TestServer::new(build_app(None));
    let response = server
        .post("/v1/compress")
        .json(&json!({
            "model": "unknown-model",
            "input": "the quick brown fox",
            "compression_settings": { "aggressiveness": 0.5 }
        }))
        .await;

    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .contains("unsupported model"));
}

// ─── Auth edge cases ─────────────────────────────────────────────────

#[tokio::test]
async fn auth_with_empty_bearer_token() {
    let server = TestServer::new(build_app(Some("secret".to_string())));
    let response = server
        .post("/v1/compress")
        .json(&json!({ "input": "hello" }))
        .add_header(
            http::header::AUTHORIZATION,
            "Bearer ".parse::<http::HeaderValue>().unwrap(),
        )
        .await;

    response.assert_status_unauthorized();
}

#[tokio::test]
async fn auth_with_basic_instead_of_bearer() {
    let server = TestServer::new(build_app(Some("secret".to_string())));
    let response = server
        .post("/v1/compress")
        .json(&json!({ "input": "hello" }))
        .add_header(
            http::header::AUTHORIZATION,
            "Basic dXNlcjpwYXNz".parse::<http::HeaderValue>().unwrap(),
        )
        .await;

    response.assert_status_unauthorized();
}

#[tokio::test]
async fn health_endpoint_bypasses_auth() {
    let server = TestServer::new(build_app(Some("secret".to_string())));
    let response = server.get("/health").await;
    response.assert_status_ok();
    response.assert_text("ok");
}

// ─── Unicode / special content ───────────────────────────────────────

#[tokio::test]
async fn unicode_input_via_api() {
    let server = TestServer::new(build_app(None));
    let response = server
        .post("/v1/compress")
        .json(&json!({
            "input": "Le café est très bon aujourd'hui",
            "compression_settings": { "aggressiveness": 0.3 }
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(!body["output"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn long_input_via_api() {
    let server = TestServer::new(build_app(None));
    let input = "the quick brown fox jumps over the lazy dog ".repeat(200);
    let response = server
        .post("/v1/compress")
        .json(&json!({
            "input": input,
            "compression_settings": { "aggressiveness": 0.5 }
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["compression_ratio"].as_f64().unwrap() < 1.0);
}

// ─── Sequential multiple requests ────────────────────────────────────

#[tokio::test]
async fn handles_multiple_sequential_requests() {
    let server = TestServer::new(build_app(None));

    for i in 0..5 {
        let response = server
            .post("/v1/compress")
            .json(&json!({
                "input": format!("request number {i} with some filler words the and a"),
                "compression_settings": { "aggressiveness": 0.5 }
            }))
            .await;
        response.assert_status_ok();
    }
}
