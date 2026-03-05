use std::sync::Arc;

use reqwest::Client;

use compress_core::Compressor;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub compressor: Arc<Compressor>,
    pub api_key: Option<String>,
    pub http_client: Client,
    pub proxy: Option<ProxyConfig>,
}

#[derive(Clone)]
pub struct ProxyConfig {
    pub upstream_base_url: String,
    pub upstream_api_key: Option<String>,
    pub aggressiveness: f32,
    pub target_model: String,
    pub min_chars: usize,
    pub only_if_smaller: bool,
}
