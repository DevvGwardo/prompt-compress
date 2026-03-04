use tiktoken_rs::{cl100k_base, CoreBPE};

use crate::error::{CompressError, Result};

/// Counts LLM tokens for a given target model.
pub struct LlmTokenCounter {
    bpe: CoreBPE,
}

impl LlmTokenCounter {
    /// Create a new token counter. Currently uses cl100k_base (GPT-4 / Claude tokenizer).
    pub fn new(_target_model: &str) -> Result<Self> {
        // cl100k_base covers GPT-4, GPT-3.5-turbo, and is a reasonable proxy for Claude
        let bpe = cl100k_base().map_err(|e| CompressError::Tokenizer(e.to_string()))?;
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
