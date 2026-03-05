use tiktoken_rs::{cl100k_base, get_bpe_from_model, o200k_base, CoreBPE};

use crate::error::{CompressError, Result};

/// Counts LLM tokens for a given target model.
pub struct LlmTokenCounter {
    bpe: CoreBPE,
}

impl LlmTokenCounter {
    /// Create a new token counter using the best available tokenizer for the target model.
    pub fn new(target_model: &str) -> Result<Self> {
        let normalized = target_model.trim().to_lowercase();

        let bpe = get_bpe_from_model(target_model)
            .or_else(|_| {
                // Common non-OpenAI aliases used by clients.
                if normalized.starts_with("claude") {
                    cl100k_base()
                } else if normalized.starts_with("gpt-5")
                    || normalized.starts_with("gpt-4.1")
                    || normalized.starts_with("gpt-4o")
                    || normalized.starts_with("o1")
                    || normalized.starts_with("o3")
                    || normalized.starts_with("o4")
                {
                    o200k_base()
                } else {
                    cl100k_base()
                }
            })
            .map_err(|e| CompressError::Tokenizer(e.to_string()))?;

        Ok(Self { bpe })
    }

    /// Count the number of tokens in the given text.
    pub fn count(&self, text: &str) -> usize {
        self.bpe.encode_ordinary(text).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_tokens() {
        let counter = LlmTokenCounter::new("gpt-4").unwrap();
        let count = counter.count("Hello, world!");
        assert!(count > 0);
        assert!(count < 10);
    }

    #[test]
    fn test_empty_string() {
        let counter = LlmTokenCounter::new("gpt-4").unwrap();
        assert_eq!(counter.count(""), 0);
    }
}
