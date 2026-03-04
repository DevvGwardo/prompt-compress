use std::sync::Arc;

use axum::{middleware as axum_mw, routing, Router};
use tower_http::{
    compression::CompressionLayer,
    decompression::RequestDecompressionLayer,
    trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;

use compress_core::{Compressor, HeuristicScorer};

mod dto;
mod middleware;
mod routes;
mod state;

use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("compress_api=info".parse()?))
        .init();

    let api_key = std::env::var("COMPRESS_API_KEY").ok();
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let bind_addr = format!("0.0.0.0:{port}");

    let scorer = HeuristicScorer::new();
    let compressor = Arc::new(Compressor::new(Box::new(scorer), "gpt-4")?);

    let state = AppState {
        compressor,
        api_key: api_key.clone(),
    };

    // Routes that require auth
    let api_routes = Router::new()
        .route("/v1/compress", routing::post(routes::compress))
        .route_layer(axum_mw::from_fn_with_state(
            state.clone(),
            middleware::auth,
        ));

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

    axum::serve(listener, app).await?;
    Ok(())
}
