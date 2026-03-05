use axum::{
    body::{to_bytes, Body},
    extract::{Path, Request, State},
    http::{
        header::{AUTHORIZATION, CONNECTION, CONTENT_LENGTH, HOST, TRANSFER_ENCODING},
        HeaderMap, StatusCode,
    },
    response::Response,
    Json,
};
use futures_util::TryStreamExt;
use serde_json::Value;

use compress_core::CompressionSettings;

use crate::dto::{CompressRequest, CompressResponse, ErrorDetail, ErrorResponse};
use crate::state::{AppState, ProxyConfig};

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

fn bad_gateway(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(ErrorResponse {
            error: ErrorDetail {
                message: message.into(),
                r#type: "upstream_error".to_string(),
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

fn compress_text(state: &AppState, cfg: &ProxyConfig, text: &str) -> Option<String> {
    if text.trim().is_empty() || text.len() < cfg.min_chars {
        return None;
    }

    let settings = CompressionSettings {
        aggressiveness: cfg.aggressiveness,
        target_model: cfg.target_model.clone(),
    };

    match state.compressor.compress(text, &settings) {
        Ok(result) => {
            if cfg.only_if_smaller && result.output_tokens >= result.original_input_tokens {
                return None;
            }
            if result.output.trim().is_empty() || result.output == text {
                return None;
            }
            Some(result.output)
        }
        Err(err) => {
            tracing::warn!("proxy compression skipped due to error: {}", err);
            None
        }
    }
}

fn compress_chat_completions_payload(
    state: &AppState,
    cfg: &ProxyConfig,
    payload: &mut Value,
) -> usize {
    let mut rewritten = 0usize;
    let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) else {
        return rewritten;
    };

    for message in messages {
        let Some(obj) = message.as_object_mut() else {
            continue;
        };

        let role = obj.get("role").and_then(Value::as_str).unwrap_or_default();
        if role != "user" {
            continue;
        }

        if let Some(content) = obj.get_mut("content") {
            if let Some(text) = content.as_str() {
                if let Some(compressed) = compress_text(state, cfg, text) {
                    *content = Value::String(compressed);
                    rewritten += 1;
                }
                continue;
            }

            if let Some(parts) = content.as_array_mut() {
                for part in parts {
                    let Some(part_obj) = part.as_object_mut() else {
                        continue;
                    };
                    let part_type = part_obj
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if part_type != "text" && part_type != "input_text" {
                        continue;
                    }
                    if let Some(text_val) = part_obj.get_mut("text") {
                        if let Some(text) = text_val.as_str() {
                            if let Some(compressed) = compress_text(state, cfg, text) {
                                *text_val = Value::String(compressed);
                                rewritten += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    rewritten
}

fn compress_responses_payload(state: &AppState, cfg: &ProxyConfig, payload: &mut Value) -> usize {
    let mut rewritten = 0usize;

    if let Some(input) = payload.get_mut("input") {
        if let Some(text) = input.as_str() {
            if let Some(compressed) = compress_text(state, cfg, text) {
                *input = Value::String(compressed);
                rewritten += 1;
            }
            return rewritten;
        }

        if let Some(items) = input.as_array_mut() {
            for item in items {
                let Some(obj) = item.as_object_mut() else {
                    continue;
                };

                let item_type = obj.get("type").and_then(Value::as_str).unwrap_or_default();
                let role = obj.get("role").and_then(Value::as_str).unwrap_or_default();

                if item_type == "input_text" {
                    if let Some(text_val) = obj.get_mut("text") {
                        if let Some(text) = text_val.as_str() {
                            if let Some(compressed) = compress_text(state, cfg, text) {
                                *text_val = Value::String(compressed);
                                rewritten += 1;
                            }
                        }
                    }
                    continue;
                }

                if role != "user" {
                    continue;
                }

                if let Some(content) = obj.get_mut("content") {
                    if let Some(text) = content.as_str() {
                        if let Some(compressed) = compress_text(state, cfg, text) {
                            *content = Value::String(compressed);
                            rewritten += 1;
                        }
                        continue;
                    }

                    if let Some(parts) = content.as_array_mut() {
                        for part in parts {
                            let Some(part_obj) = part.as_object_mut() else {
                                continue;
                            };
                            let part_type = part_obj
                                .get("type")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            if part_type != "text" && part_type != "input_text" {
                                continue;
                            }

                            if let Some(text_val) = part_obj.get_mut("text") {
                                if let Some(text) = text_val.as_str() {
                                    if let Some(compressed) = compress_text(state, cfg, text) {
                                        *text_val = Value::String(compressed);
                                        rewritten += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    rewritten
}

fn rewrite_proxy_payload(state: &AppState, path: &str, body: &[u8]) -> Option<Vec<u8>> {
    let cfg = state.proxy.as_ref()?;
    let mut payload: Value = serde_json::from_slice(body).ok()?;
    let rewritten = match path {
        "chat/completions" => compress_chat_completions_payload(state, cfg, &mut payload),
        "responses" => compress_responses_payload(state, cfg, &mut payload),
        _ => 0,
    };

    if rewritten == 0 {
        return None;
    }

    tracing::info!(
        "proxy compressed {} request text block(s) for path {}",
        rewritten,
        path
    );
    serde_json::to_vec(&payload).ok()
}

fn should_rewrite_json(headers: &HeaderMap) -> bool {
    headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase().contains("application/json"))
        .unwrap_or(false)
}

fn sanitize_proxy_headers(headers: &HeaderMap, has_upstream_api_key: bool) -> HeaderMap {
    let mut sanitized = HeaderMap::new();
    for (name, value) in headers {
        if name == HOST || name == CONTENT_LENGTH || name == CONNECTION {
            continue;
        }
        if has_upstream_api_key && name == AUTHORIZATION {
            continue;
        }
        sanitized.append(name.clone(), value.clone());
    }
    sanitized
}

/// ANY /v1/proxy/{*path}
pub async fn proxy(
    Path(path): Path<String>,
    State(state): State<AppState>,
    req: Request,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let Some(proxy_cfg) = state.proxy.as_ref() else {
        return Err(bad_gateway(
            "proxy is disabled; set COMPRESS_PROXY_UPSTREAM_BASE_URL",
        ));
    };

    let method = req.method().clone();
    let headers = req.headers().clone();
    let body = to_bytes(req.into_body(), 50 * 1024 * 1024)
        .await
        .map_err(|e| bad_request(format!("failed to read request body: {e}")))?;

    let mut outbound_body = body.to_vec();
    if method == axum::http::Method::POST && should_rewrite_json(&headers) {
        if let Some(rewritten) = rewrite_proxy_payload(&state, &path, body.as_ref()) {
            outbound_body = rewritten;
        }
    }

    let path = path.trim_start_matches('/');
    let upstream_url = format!(
        "{}/{}",
        proxy_cfg.upstream_base_url.trim_end_matches('/'),
        path
    );

    let method = reqwest::Method::from_bytes(method.as_str().as_bytes()).map_err(|e| {
        bad_request(format!(
            "unsupported HTTP method '{}': {}",
            method.as_str(),
            e
        ))
    })?;

    let mut upstream_req = state
        .http_client
        .request(method, upstream_url)
        .headers(sanitize_proxy_headers(
            &headers,
            proxy_cfg.upstream_api_key.is_some(),
        ))
        .body(outbound_body);

    if let Some(ref key) = proxy_cfg.upstream_api_key {
        upstream_req = upstream_req.bearer_auth(key);
    }

    let upstream_resp = upstream_req
        .send()
        .await
        .map_err(|e| bad_gateway(format!("failed to reach upstream: {e}")))?;

    let status =
        StatusCode::from_u16(upstream_resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let upstream_headers = upstream_resp.headers().clone();
    let upstream_stream = upstream_resp
        .bytes_stream()
        .map_err(|e| std::io::Error::other(format!("upstream stream error: {}", e)));

    let mut response = Response::new(Body::from_stream(upstream_stream));
    *response.status_mut() = status;
    for (name, value) in &upstream_headers {
        if name == CONTENT_LENGTH || name == TRANSFER_ENCODING || name == CONNECTION {
            continue;
        }
        response.headers_mut().append(name.clone(), value.clone());
    }

    Ok(response)
}

/// GET /health
pub async fn health() -> &'static str {
    "ok"
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{extract::Request, response::IntoResponse, routing, Router};
    use axum_test::TestServer;
    use compress_core::{Compressor, HeuristicScorer};
    use http::StatusCode;
    use serde_json::json;

    use crate::state::{AppState, ProxyConfig};

    fn test_state(upstream_base_url: &str) -> AppState {
        let compressor =
            Compressor::new(Box::new(HeuristicScorer::new()), "gpt-4").expect("compressor");
        AppState {
            compressor: Arc::new(compressor),
            api_key: None,
            http_client: reqwest::Client::new(),
            proxy: Some(ProxyConfig {
                upstream_base_url: upstream_base_url.to_string(),
                upstream_api_key: None,
                aggressiveness: 0.8,
                target_model: "gpt-4".to_string(),
                min_chars: 1,
                only_if_smaller: false,
            }),
        }
    }

    #[test]
    fn rewrites_chat_completions_user_message_only() {
        let state = test_state("http://localhost:1234");
        let payload = json!({
            "model": "gpt-4o-mini",
            "messages": [
                {"role": "system", "content": "Do not change this."},
                {"role": "user", "content": "Please provide a concise migration plan with rollback details."},
                {"role": "assistant", "content": "Previous answer"}
            ]
        });

        let out = super::rewrite_proxy_payload(
            &state,
            "chat/completions",
            serde_json::to_vec(&payload).expect("serialize").as_slice(),
        )
        .expect("payload should be rewritten");

        let rewritten: serde_json::Value = serde_json::from_slice(&out).expect("json");
        let user_content = rewritten["messages"][1]["content"]
            .as_str()
            .expect("user content");
        let assistant_content = rewritten["messages"][2]["content"]
            .as_str()
            .expect("assistant content");

        assert_ne!(
            user_content,
            payload["messages"][1]["content"]
                .as_str()
                .expect("orig user")
        );
        assert_eq!(
            assistant_content,
            payload["messages"][2]["content"]
                .as_str()
                .expect("orig assistant")
        );
    }

    #[test]
    fn rewrites_responses_input_text_blocks() {
        let state = test_state("http://localhost:1234");
        let payload = json!({
            "model": "gpt-4.1-mini",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        {"type": "input_text", "text": "Draft a weekly status update and include risks and blockers."}
                    ]
                }
            ]
        });

        let out = super::rewrite_proxy_payload(
            &state,
            "responses",
            serde_json::to_vec(&payload).expect("serialize").as_slice(),
        )
        .expect("payload should be rewritten");

        let rewritten: serde_json::Value = serde_json::from_slice(&out).expect("json");
        let text = rewritten["input"][0]["content"][0]["text"]
            .as_str()
            .expect("rewritten text");
        assert_ne!(
            text,
            payload["input"][0]["content"][0]["text"]
                .as_str()
                .expect("orig text")
        );
    }

    async fn echo_body(req: Request) -> impl IntoResponse {
        let body = req.into_body();
        (StatusCode::OK, body)
    }

    #[tokio::test]
    async fn proxy_route_rewrites_and_forwards_chat_payload() {
        let upstream = Router::new().route("/chat/completions", routing::post(echo_body));
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind upstream");
        let upstream_addr = upstream_listener.local_addr().expect("upstream addr");
        tokio::spawn(async move {
            axum::serve(upstream_listener, upstream)
                .await
                .expect("serve upstream");
        });

        let state = test_state(&format!("http://{}", upstream_addr));
        let app = Router::new()
            .route("/v1/proxy/{*path}", routing::any(super::proxy))
            .with_state(state);
        let server = TestServer::new(app);

        let original =
            "Please provide a detailed migration plan with rollback and validation steps.";
        let response = server
            .post("/v1/proxy/chat/completions")
            .json(&json!({
                "model": "gpt-4o-mini",
                "messages": [
                    {"role": "user", "content": original}
                ]
            }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        let forwarded = body["messages"][0]["content"]
            .as_str()
            .expect("forwarded text");

        assert_ne!(forwarded, original);
    }
}
