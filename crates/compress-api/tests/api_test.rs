use axum::{middleware as axum_mw, response::IntoResponse, routing, Router};
use axum_test::TestServer;
use serde_json::json;
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::decompression::RequestDecompressionLayer;

use compress_core::{Compressor, HeuristicScorer};

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
            axum::Json(json!({"error": {"message": e.to_string()}})),
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

#[tokio::test]
async fn test_health_check() {
    let server = TestServer::new(build_app(None));
    let response = server.get("/health").await;
    response.assert_status_ok();
    response.assert_text("ok");
}

#[tokio::test]
async fn test_compress_no_auth() {
    let server = TestServer::new(build_app(None));

    let response = server
        .post("/v1/compress")
        .json(&json!({
            "input": "the quick brown fox jumps over the lazy dog",
            "compression_settings": { "aggressiveness": 0.5 }
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["output"].is_string());
    assert!(body["output_tokens"].is_number());
    assert!(body["original_input_tokens"].is_number());
    assert!(body["compression_ratio"].is_number());
    assert!(body["compression_ratio"].as_f64().unwrap() <= 1.0);
}

#[tokio::test]
async fn test_compress_with_auth() {
    let server = TestServer::new(build_app(Some("test-key-123".to_string())));

    // Without auth header → 401
    let response = server
        .post("/v1/compress")
        .json(&json!({"input": "hello world"}))
        .await;
    response.assert_status_unauthorized();

    // With wrong key → 401
    let response = server
        .post("/v1/compress")
        .json(&json!({"input": "hello world"}))
        .add_header(
            http::header::AUTHORIZATION,
            "Bearer wrong-key".parse::<http::HeaderValue>().unwrap(),
        )
        .await;
    response.assert_status_unauthorized();

    // With correct key → 200
    let response = server
        .post("/v1/compress")
        .json(&json!({"input": "hello world"}))
        .add_header(
            http::header::AUTHORIZATION,
            "Bearer test-key-123".parse::<http::HeaderValue>().unwrap(),
        )
        .await;
    response.assert_status_ok();
}

#[tokio::test]
async fn test_compress_empty_input() {
    let server = TestServer::new(build_app(None));

    let response = server
        .post("/v1/compress")
        .json(&json!({
            "input": "",
            "compression_settings": { "aggressiveness": 0.5 }
        }))
        .await;

    response.assert_status_bad_request();
}

#[tokio::test]
async fn test_compress_high_aggressiveness() {
    let server = TestServer::new(build_app(None));

    let response = server
        .post("/v1/compress")
        .json(&json!({
            "input": "the quick brown fox jumps over the lazy dog and it was a very good day for everyone",
            "compression_settings": { "aggressiveness": 0.8 }
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let ratio = body["compression_ratio"].as_f64().unwrap();
    assert!(ratio < 1.0, "should compress with high aggressiveness");
}
