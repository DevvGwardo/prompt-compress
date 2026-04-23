use std::sync::Arc;

use axum::{middleware as axum_mw, routing, Router};
use tower_http::{
    compression::CompressionLayer, decompression::RequestDecompressionLayer, trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;

use compress_core::{Compressor, HeuristicScorer};

mod dto;
mod middleware;
mod routes;
mod state;

use state::AppState;
use state::ProxyConfig;

fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(
            v.as_str(),
            "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
        ),
        Err(_) => default,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("compress_api=info".parse()?))
        .init();

    let api_key = std::env::var("COMPRESS_API_KEY").ok();
    let host = std::env::var("COMPRESS_API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let bind_addr = format!("{host}:{port}");
    let proxy_upstream_base_url = std::env::var("COMPRESS_PROXY_UPSTREAM_BASE_URL").ok();
    let proxy_upstream_api_key = std::env::var("COMPRESS_PROXY_UPSTREAM_API_KEY").ok();
    let proxy_aggressiveness = std::env::var("COMPRESS_PROXY_AGGRESSIVENESS")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.4);
    let proxy_target_model =
        std::env::var("COMPRESS_PROXY_TARGET_MODEL").unwrap_or_else(|_| "gpt-4".to_string());
    let proxy_min_chars = std::env::var("COMPRESS_PROXY_MIN_CHARS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(80);
    let proxy_only_if_smaller = env_bool("COMPRESS_PROXY_ONLY_IF_SMALLER", true);
    let proxy_scorer_mode = std::env::var("COMPRESS_PROXY_SCORER_MODE")
        .ok()
        .and_then(|v| match v.as_str() {
            "agent-aware" => Some(compress_core::HeuristicMode::AgentAware),
            _ => Some(compress_core::HeuristicMode::Standard),
        })
        .unwrap_or(compress_core::HeuristicMode::Standard);
    let http_client = reqwest::Client::builder().build()?;

    let scorer = HeuristicScorer::with_mode(proxy_scorer_mode);
    let compressor = Arc::new(Compressor::new(Box::new(scorer), &proxy_target_model)?);
    let proxy = proxy_upstream_base_url.map(|upstream_base_url| ProxyConfig {
        upstream_base_url,
        upstream_api_key: proxy_upstream_api_key,
        aggressiveness: proxy_aggressiveness,
        target_model: proxy_target_model,
        min_chars: proxy_min_chars,
        only_if_smaller: proxy_only_if_smaller,
        scorer_mode: proxy_scorer_mode,
    });

    let state = AppState {
        compressor,
        api_key: api_key.clone(),
        http_client,
        proxy: proxy.clone(),
    };

    // Routes that require auth
    let api_routes = Router::new()
        .route("/v1/compress", routing::post(routes::compress))
        .route("/v1/compress/preset/{name}", routing::post(routes::compress_preset))
        .route("/v1/compress/detect", routing::post(routes::compress_detect))
        .route("/v1/proxy/{*path}", routing::any(routes::proxy))
        .route_layer(axum_mw::from_fn_with_state(state.clone(), middleware::auth));

    let app = Router::new()
        .merge(api_routes)
        .route("/health", routing::get(routes::health))
        .layer(CompressionLayer::new())
        .layer(RequestDecompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("listening on {bind_addr}");

    if api_key.is_some() {
        tracing::info!("bearer token auth enabled");
    } else {
        tracing::warn!("no COMPRESS_API_KEY set — auth disabled");
    }

    if let Some(proxy_cfg) = proxy {
        tracing::info!(
            "proxy enabled for /v1/proxy/*path -> {} (min_chars={}, only_if_smaller={})",
            proxy_cfg.upstream_base_url,
            proxy_cfg.min_chars,
            proxy_cfg.only_if_smaller
        );
    } else {
        tracing::info!(
            "proxy disabled; set COMPRESS_PROXY_UPSTREAM_BASE_URL to enable /v1/proxy/*path"
        );
    }

    axum::serve(listener, app).await?;
    Ok(())
}
