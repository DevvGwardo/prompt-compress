use std::sync::Arc;

use compress_core::Compressor;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub compressor: Arc<Compressor>,
    pub api_key: Option<String>,
}
