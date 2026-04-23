use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use reqwest::Client;

use compress_core::Compressor;
use crate::dto::MetricsEntry;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub compressor: Arc<Compressor>,
    pub api_key: Option<String>,
    pub http_client: Client,
    pub proxy: Option<ProxyConfig>,
    pub metrics: Arc<Mutex<HashMap<String, MetricsEntry>>>,
}

#[derive(Clone)]
pub struct ProxyConfig {
    pub upstream_base_url: String,
    pub upstream_api_key: Option<String>,
    pub aggressiveness: f32,
    pub target_model: String,
    pub min_chars: usize,
    pub only_if_smaller: bool,
    pub scorer_mode: compress_core::scorer::HeuristicMode,
}
