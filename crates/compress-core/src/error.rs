use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompressError {
    #[error("tokenizer error: {0}")]
    Tokenizer(String),

    #[error("model error: {0}")]
    Model(String),

    #[error("invalid aggressiveness {0}: must be between 0.0 and 1.0")]
    InvalidAggressiveness(f32),

    #[error("input is empty")]
    EmptyInput,

    #[error("config error: {0}")]
    Config(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, CompressError>;
