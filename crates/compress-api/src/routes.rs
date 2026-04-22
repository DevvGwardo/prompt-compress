use axum::{
    body::{to_bytes, Body},
    extract::{Path, Request, State},
    http::{
        header::{
            AUTHORIZATION, CONNECTION, CONTENT_ENCODING, CONTENT_LENGTH, HOST, TRANSFER_ENCODING,
        },
        HeaderMap, StatusCode,
    },
    response::Response,
    Json,
};
use futures_util::TryStreamExt;
use serde_json::Value;

use compress_core::{CompressionSettings, HeuristicMode};

use crate::dto::{CompressPresetRequest, CompressPresetResponse, CompressRequest, CompressResponse, ErrorDetail, ErrorResponse};
use crate::state::{AppState, ProxyConfig};

#[derive(Default)]
struct RewriteStats {
    attempted_blocks: usize,
    rewritten_blocks: usize,
    original_tokens: usize,
    output_tokens: usize,
}

impl RewriteStats {
    fn record(&mut self, original_tokens: usize, output_tokens: usize, rewritten: bool) {
        self.attempted_blocks += 1;
        self.original_tokens += original_tokens;
        self.output_tokens += output_tokens;
        if rewritten {
            self.rewritten_blocks += 1;
        }
    }

    fn saved_tokens(&self) -> usize {
        self.original_tokens.saturating_sub(self.output_tokens)
    }
}

struct CompressionAttempt {
    output: String,
    output_tokens: usize,
    original_input_tokens: usize,
}

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
    if req.model != "scorer-v0.1" && req.model != "heuristic-v0.1" && req.model != "heuristic-agent-v0.1" {
        return Err(bad_request(format!(
            "unsupported model '{}'; supported values: scorer-v0.1, heuristic-v0.1, heuristic-agent-v0.1",
            req.model
        )));
    }

    let settings = CompressionSettings {
        aggressiveness: req.compression_settings.aggressiveness,
        target_model: req.compression_settings.target_model,
        scorer_mode: if req.model == "heuristic-agent-v0.1" {
            HeuristicMode::AgentAware
        } else {
            HeuristicMode::Standard
        },
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

fn preset_aggressiveness(preset: &str) -> Option<f32> {
    match preset {
        "system" => Some(0.3),
        "context" => Some(0.5),
        "tools" => Some(0.2),
        "memory" => Some(0.6),
        _ => None,
    }
}

/// POST /v1/compress/preset/:name
pub async fn compress_preset(
    State(state): State<AppState>,
    Path(preset): Path<String>,
    Json(req): Json<CompressPresetRequest>,
) -> Result<Json<CompressPresetResponse>, (StatusCode, Json<ErrorResponse>)> {
    let aggressiveness = match preset_aggressiveness(&preset) {
        Some(a) => a,
        None => {
            return Err(bad_request(format!(
                "unsupported preset '{}'; supported values: system, context, tools, memory",
                preset
            )));
        }
    };

    let settings = CompressionSettings {
        aggressiveness,
        target_model: req.target_model,
        scorer_mode: HeuristicMode::AgentAware,
        ..Default::default()
    };

    match state.compressor.compress(&req.input, &settings) {
        Ok(result) => Ok(Json(CompressPresetResponse {
            preset: preset.clone(),
            output: result.output,
            output_tokens: result.output_tokens,
            original_input_tokens: result.original_input_tokens,
            compression_ratio: result.compression_ratio,
        })),
        Err(e) => Err(bad_request(e.to_string())),
    }
}

fn compress_text(state: &AppState, cfg: &ProxyConfig, text: &str) -> Option<CompressionAttempt> {
    if text.trim().is_empty() {
        return None;
    }

    let settings = CompressionSettings {
        aggressiveness: cfg.aggressiveness,
        target_model: cfg.target_model.clone(),
        scorer_mode: cfg.scorer_mode,
    };

    match state.compressor.compress(text, &settings) {
        Ok(result) => Some(CompressionAttempt {
            output: result.output,
            output_tokens: result.output_tokens,
            original_input_tokens: result.original_input_tokens,
        }),
        Err(err) => {
            tracing::warn!("proxy compression skipped due to error: {}", err);
            None
        }
    }
}

fn should_rewrite_text(cfg: &ProxyConfig, original: &str, attempt: &CompressionAttempt) -> bool {
    if original.len() < cfg.min_chars {
        return false;
    }
    if cfg.only_if_smaller && attempt.output_tokens >= attempt.original_input_tokens {
        return false;
    }
    !attempt.output.trim().is_empty() && attempt.output != original
}

fn compress_chat_completions_payload(
    state: &AppState,
    cfg: &ProxyConfig,
    payload: &mut Value,
) -> RewriteStats {
    let mut stats = RewriteStats::default();
    let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) else {
        return stats;
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
                if let Some(attempt) = compress_text(state, cfg, text) {
                    let rewritten = should_rewrite_text(cfg, text, &attempt);
                    stats.record(
                        attempt.original_input_tokens,
                        attempt.output_tokens,
                        rewritten,
                    );
                    if rewritten {
                        *content = Value::String(attempt.output);
                    }
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
                            if let Some(attempt) = compress_text(state, cfg, text) {
                                let rewritten = should_rewrite_text(cfg, text, &attempt);
                                stats.record(
                                    attempt.original_input_tokens,
                                    attempt.output_tokens,
                                    rewritten,
                                );
                                if rewritten {
                                    *text_val = Value::String(attempt.output);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    stats
}

fn compress_responses_payload(
    state: &AppState,
    cfg: &ProxyConfig,
    payload: &mut Value,
) -> RewriteStats {
    let mut stats = RewriteStats::default();

    if let Some(input) = payload.get_mut("input") {
        if let Some(text) = input.as_str() {
            if let Some(attempt) = compress_text(state, cfg, text) {
                let rewritten = should_rewrite_text(cfg, text, &attempt);
                stats.record(
                    attempt.original_input_tokens,
                    attempt.output_tokens,
                    rewritten,
                );
                if rewritten {
                    *input = Value::String(attempt.output);
                }
            }
            return stats;
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
                            if let Some(attempt) = compress_text(state, cfg, text) {
                                let rewritten = should_rewrite_text(cfg, text, &attempt);
                                stats.record(
                                    attempt.original_input_tokens,
                                    attempt.output_tokens,
                                    rewritten,
                                );
                                if rewritten {
                                    *text_val = Value::String(attempt.output);
                                }
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
                        if let Some(attempt) = compress_text(state, cfg, text) {
                            let rewritten = should_rewrite_text(cfg, text, &attempt);
                            stats.record(
                                attempt.original_input_tokens,
                                attempt.output_tokens,
                                rewritten,
                            );
                            if rewritten {
                                *content = Value::String(attempt.output);
                            }
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
                                    if let Some(attempt) = compress_text(state, cfg, text) {
                                        let rewritten = should_rewrite_text(cfg, text, &attempt);
                                        stats.record(
                                            attempt.original_input_tokens,
                                            attempt.output_tokens,
                                            rewritten,
                                        );
                                        if rewritten {
                                            *text_val = Value::String(attempt.output);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    stats
}

fn compress_codex_message_array(
    state: &AppState,
    cfg: &ProxyConfig,
    messages: &mut Vec<Value>,
    stats: &mut RewriteStats,
) {
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
                if let Some(attempt) = compress_text(state, cfg, text) {
                    let rewritten = should_rewrite_text(cfg, text, &attempt);
                    stats.record(
                        attempt.original_input_tokens,
                        attempt.output_tokens,
                        rewritten,
                    );
                    if rewritten {
                        *content = Value::String(attempt.output);
                    }
                }
            }
        }
    }
}

fn compress_codex_backend_payload(
    state: &AppState,
    cfg: &ProxyConfig,
    payload: &mut Value,
) -> RewriteStats {
    let mut stats = RewriteStats::default();

    if let Some(prompt) = payload.get_mut("prompt") {
        if let Some(text) = prompt.as_str() {
            if let Some(attempt) = compress_text(state, cfg, text) {
                let rewritten = should_rewrite_text(cfg, text, &attempt);
                stats.record(
                    attempt.original_input_tokens,
                    attempt.output_tokens,
                    rewritten,
                );
                if rewritten {
                    *prompt = Value::String(attempt.output);
                }
            }
        }
    }

    if let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) {
        compress_codex_message_array(state, cfg, messages, &mut stats);
    }

    if let Some(messages) = payload
        .get_mut("initial_messages")
        .and_then(Value::as_array_mut)
    {
        compress_codex_message_array(state, cfg, messages, &mut stats);
    }

    stats
}

fn is_chat_completions_path(path: &str) -> bool {
    path == "chat/completions" || path.ends_with("/chat/completions")
}

fn is_responses_path(path: &str) -> bool {
    path == "responses" || path.ends_with("/responses")
}

fn is_anthropic_messages_path(path: &str) -> bool {
    path == "messages" || path.ends_with("/messages")
}

/// Compress Anthropic Messages API payloads.
/// Format: { "messages": [{"role": "user", "content": "..."}] }
/// Content can be a string or an array of content blocks like [{"type": "text", "text": "..."}].
fn compress_anthropic_messages_payload(
    state: &AppState,
    cfg: &ProxyConfig,
    payload: &mut Value,
) -> RewriteStats {
    let mut stats = RewriteStats::default();
    let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) else {
        return stats;
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
            // String content: {"role": "user", "content": "text here"}
            if let Some(text) = content.as_str() {
                if let Some(attempt) = compress_text(state, cfg, text) {
                    let rewritten = should_rewrite_text(cfg, text, &attempt);
                    stats.record(
                        attempt.original_input_tokens,
                        attempt.output_tokens,
                        rewritten,
                    );
                    if rewritten {
                        *content = Value::String(attempt.output);
                    }
                }
                continue;
            }

            // Array content: {"role": "user", "content": [{"type": "text", "text": "..."}]}
            if let Some(parts) = content.as_array_mut() {
                for part in parts {
                    let Some(part_obj) = part.as_object_mut() else {
                        continue;
                    };
                    let part_type = part_obj
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if part_type != "text" {
                        continue;
                    }
                    if let Some(text_val) = part_obj.get_mut("text") {
                        if let Some(text) = text_val.as_str() {
                            if let Some(attempt) = compress_text(state, cfg, text) {
                                let rewritten = should_rewrite_text(cfg, text, &attempt);
                                stats.record(
                                    attempt.original_input_tokens,
                                    attempt.output_tokens,
                                    rewritten,
                                );
                                if rewritten {
                                    *text_val = Value::String(attempt.output);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    stats
}

fn rewrite_proxy_payload(state: &AppState, path: &str, body: &[u8]) -> Option<Vec<u8>> {
    let cfg = state.proxy.as_ref()?;
    let mut payload: Value = serde_json::from_slice(body).ok()?;
    let stats = if path.contains("api/codex") {
        compress_codex_backend_payload(state, cfg, &mut payload)
    } else if is_chat_completions_path(path) {
        compress_chat_completions_payload(state, cfg, &mut payload)
    } else if is_responses_path(path) {
        compress_responses_payload(state, cfg, &mut payload)
    } else if is_anthropic_messages_path(path) {
        compress_anthropic_messages_payload(state, cfg, &mut payload)
    } else {
        RewriteStats::default()
    };

    if stats.attempted_blocks == 0 {
        tracing::info!("proxy saw JSON request with no rewriteable text blocks for path {}", path);
        return None;
    }

    tracing::info!(
        "proxy stats path={} attempted_blocks={} rewritten_blocks={} tokens={} -> {} saved={} ratio={:.1}%",
        path,
        stats.attempted_blocks,
        stats.rewritten_blocks,
        stats.original_tokens,
        stats.output_tokens,
        stats.saved_tokens(),
        if stats.original_tokens == 0 {
            0.0
        } else {
            100.0 * (stats.saved_tokens() as f64) / (stats.original_tokens as f64)
        }
    );
    if stats.rewritten_blocks == 0 {
        return None;
    }

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
        if name == HOST
            || name == CONTENT_LENGTH
            || name == CONNECTION
            || name == CONTENT_ENCODING
        {
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
    let uri = req.uri().clone();
    let headers = req.headers().clone();
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    tracing::info!(
        "proxy request method={} path={} content-type={}",
        method.as_str(),
        path,
        content_type
    );
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
    let mut upstream_url = format!(
        "{}/{}",
        proxy_cfg.upstream_base_url.trim_end_matches('/'),
        path
    );
    if let Some(query) = uri.query() {
        upstream_url.push('?');
        upstream_url.push_str(query);
    }

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
                scorer_mode: compress_core::HeuristicMode::Standard,
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

    #[test]
    fn rewrites_anthropic_messages_string_content() {
        let state = test_state("http://localhost:1234");
        let payload = json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": "Please provide a detailed migration plan with rollback and validation steps."}
            ]
        });

        let out = super::rewrite_proxy_payload(
            &state,
            "messages",
            serde_json::to_vec(&payload).expect("serialize").as_slice(),
        )
        .expect("payload should be rewritten");

        let rewritten: serde_json::Value = serde_json::from_slice(&out).expect("json");
        let user_content = rewritten["messages"][0]["content"]
            .as_str()
            .expect("user content");

        assert_ne!(
            user_content,
            payload["messages"][0]["content"]
                .as_str()
                .expect("orig user")
        );
    }

    #[test]
    fn rewrites_anthropic_messages_array_content() {
        let state = test_state("http://localhost:1234");
        let payload = json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "Draft a weekly status update and include risks and blockers."}
                    ]
                }
            ]
        });

        let out = super::rewrite_proxy_payload(
            &state,
            "v1/messages",
            serde_json::to_vec(&payload).expect("serialize").as_slice(),
        )
        .expect("payload should be rewritten");

        let rewritten: serde_json::Value = serde_json::from_slice(&out).expect("json");
        let text = rewritten["messages"][0]["content"][0]["text"]
            .as_str()
            .expect("rewritten text");
        assert_ne!(
            text,
            payload["messages"][0]["content"][0]["text"]
                .as_str()
                .expect("orig text")
        );
    }

    #[test]
    fn skips_anthropic_messages_assistant_role() {
        let state = test_state("http://localhost:1234");
        let payload = json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": "Please provide a detailed migration plan with rollback and validation steps."},
                {"role": "assistant", "content": "Here is the plan."}
            ]
        });

        let out = super::rewrite_proxy_payload(
            &state,
            "messages",
            serde_json::to_vec(&payload).expect("serialize").as_slice(),
        )
        .expect("payload should be rewritten");

        let rewritten: serde_json::Value = serde_json::from_slice(&out).expect("json");
        let assistant_content = rewritten["messages"][1]["content"]
            .as_str()
            .expect("assistant content");
        assert_eq!(assistant_content, "Here is the plan.");
    }

    #[test]
    fn rewrites_codex_responses_path_like_standard_responses() {
        let state = test_state("http://localhost:1234");
        let payload = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        {"type": "input_text", "text": "Write a large PRD with requirements, rollout phases, and edge cases."}
                    ]
                }
            ],
            "stream": true
        });

        let out = super::rewrite_proxy_payload(
            &state,
            "codex/responses",
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

    #[tokio::test]
    async fn proxy_route_rewrites_and_forwards_codex_responses_payload() {
        let upstream = Router::new().route("/codex/responses", routing::post(echo_body));
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

        let original = "Please write a comprehensive PRD with detailed milestones, constraints, and risks.";
        let response = server
            .post("/v1/proxy/codex/responses")
            .json(&json!({
                "model": "gpt-5.4",
                "input": [
                    {
                        "type": "message",
                        "role": "user",
                        "content": [
                            {"type": "input_text", "text": original}
                        ]
                    }
                ],
                "stream": true
            }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        let forwarded = body["input"][0]["content"][0]["text"]
            .as_str()
            .expect("forwarded text");

        assert_ne!(forwarded, original);
    }

    fn preset_test_state() -> AppState {
        let compressor =
            Compressor::new(Box::new(HeuristicScorer::new()), "gpt-4").expect("compressor");
        AppState {
            compressor: Arc::new(compressor),
            api_key: None,
            http_client: reqwest::Client::new(),
            proxy: None,
        }
    }

    #[tokio::test]
    async fn preset_system_compresses_with_low_aggressiveness() {
        let state = preset_test_state();
        let app = Router::new()
            .route("/v1/compress/preset/{name}", routing::post(super::compress_preset))
            .with_state(state);
        let server = TestServer::new(app);

        let original = "Please create a detailed migration plan with rollback and validation steps for the database.";
        let response = server
            .post("/v1/compress/preset/system")
            .json(&json!({ "input": original }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["preset"].as_str().unwrap(), "system");
        let output = body["output"].as_str().unwrap();
        assert_ne!(output, original);
        let ratio = body["compression_ratio"].as_f64().unwrap();
        assert!(ratio < 1.0);
    }

    #[tokio::test]
    async fn preset_memory_compresses_aggressively() {
        let state = preset_test_state();
        let app = Router::new()
            .route("/v1/compress/preset/{name}", routing::post(super::compress_preset))
            .with_state(state);
        let server = TestServer::new(app);

        let original = "Hey there, I just wanted to say thanks for all the help yesterday. It was really great working with you on the project.";
        let response = server
            .post("/v1/compress/preset/memory")
            .json(&json!({ "input": original }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["preset"].as_str().unwrap(), "memory");
        let ratio = body["compression_ratio"].as_f64().unwrap();
        assert!(ratio < 1.0);
    }

    #[tokio::test]
    async fn preset_invalid_returns_400() {
        let state = preset_test_state();
        let app = Router::new()
            .route("/v1/compress/preset/{name}", routing::post(super::compress_preset))
            .with_state(state);
        let server = TestServer::new(app);

        let response = server
            .post("/v1/compress/preset/invalid")
            .json(&json!({ "input": "hello world" }))
            .await;

        response.assert_status_bad_request();
        let body: serde_json::Value = response.json();
        assert!(body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unsupported preset"));
    }

    #[tokio::test]
    async fn preset_uses_target_model_from_request() {
        let state = preset_test_state();
        let app = Router::new()
            .route("/v1/compress/preset/{name}", routing::post(super::compress_preset))
            .with_state(state);
        let server = TestServer::new(app);

        let response = server
            .post("/v1/compress/preset/tools")
            .json(&json!({
                "input": "the quick brown fox jumps over the lazy dog",
                "target_model": "claude-3-opus"
            }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["preset"].as_str().unwrap(), "tools");
        assert!(body["output_tokens"].is_number());
    }
}
