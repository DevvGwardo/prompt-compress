use regex::Regex;

use crate::config::{CompressionResult, CompressionSettings};
use crate::error::{CompressError, Result};
use crate::scorer::{ScoredToken, TokenScorer};
use crate::tokenizer::LlmTokenCounter;

/// Main compressor that orchestrates scoring, filtering, and reconstruction.
pub struct Compressor {
    scorer: Box<dyn TokenScorer>,
    counter: LlmTokenCounter,
}

impl Compressor {
    pub fn new(scorer: Box<dyn TokenScorer>, target_model: &str) -> Result<Self> {
        let counter = LlmTokenCounter::new(target_model)?;
        Ok(Self { scorer, counter })
    }

    /// Compress the input text according to the given settings.
    pub fn compress(&self, input: &str, settings: &CompressionSettings) -> Result<CompressionResult> {
        if input.trim().is_empty() {
            return Err(CompressError::EmptyInput);
        }

        if settings.aggressiveness < 0.0 || settings.aggressiveness > 1.0 {
            return Err(CompressError::InvalidAggressiveness(settings.aggressiveness));
        }

        let original_tokens = self.counter.count(input);

        // If aggressiveness is 0, return input unchanged
        if settings.aggressiveness == 0.0 {
            return Ok(CompressionResult {
                output: input.to_string(),
                output_tokens: original_tokens,
                original_input_tokens: original_tokens,
                compression_ratio: 1.0,
            });
        }

        // Extract safe regions before scoring
        let (cleaned_input, safe_regions) = extract_safe_regions(input);

        // Score tokens
        let mut scored = self.scorer.score(&cleaned_input)?;

        // Mark protected tokens
        mark_protected_tokens(&mut scored, &safe_regions);

        // Filter based on threshold: higher aggressiveness → higher threshold → more tokens removed
        let threshold = settings.aggressiveness;
        let output = reconstruct(&scored, threshold);

        let output_tokens = self.counter.count(&output);
        let compression_ratio = if original_tokens > 0 {
            output_tokens as f64 / original_tokens as f64
        } else {
            1.0
        };

        Ok(CompressionResult {
            output,
            output_tokens,
            original_input_tokens: original_tokens,
            compression_ratio,
        })
    }
}

/// Extract `<ttc_safe>...</ttc_safe>` regions and return cleaned text + safe word sets.
fn extract_safe_regions(input: &str) -> (String, Vec<String>) {
    let re = Regex::new(r"<ttc_safe>(.*?)</ttc_safe>").unwrap();
    let mut safe_words = Vec::new();

    for cap in re.captures_iter(input) {
        if let Some(content) = cap.get(1) {
            // Collect each word inside safe tags
            for word in content.as_str().split_whitespace() {
                safe_words.push(word.to_string());
            }
        }
    }

    // Remove the tags but keep the content
    let cleaned = re.replace_all(input, "$1").to_string();
    (cleaned, safe_words)
}

/// Mark tokens that fall within safe regions as protected.
fn mark_protected_tokens(tokens: &mut [ScoredToken], safe_words: &[String]) {
    for token in tokens.iter_mut() {
        if safe_words.contains(&token.text) {
            token.protected = true;
            token.importance = 1.0;
        }
    }
}

/// Reconstruct text from scored tokens, keeping those above the threshold.
fn reconstruct(tokens: &[ScoredToken], threshold: f32) -> String {
    let kept: Vec<&str> = tokens
        .iter()
        .filter(|t| t.protected || t.importance >= threshold)
        .map(|t| t.text.as_str())
        .collect();

    kept.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scorer::HeuristicScorer;

    fn make_compressor() -> Compressor {
        Compressor::new(Box::new(HeuristicScorer::new()), "gpt-4").unwrap()
    }

    #[test]
    fn test_no_compression() {
        let c = make_compressor();
        let settings = CompressionSettings {
            aggressiveness: 0.0,
            ..Default::default()
        };
        let result = c.compress("Hello world", &settings).unwrap();
        assert_eq!(result.output, "Hello world");
        assert_eq!(result.compression_ratio, 1.0);
    }

    #[test]
    fn test_basic_compression() {
        let c = make_compressor();
        let settings = CompressionSettings {
            aggressiveness: 0.5,
            ..Default::default()
        };
        let input = "the quick brown fox jumps over the lazy dog";
        let result = c.compress(input, &settings).unwrap();

        // Should remove some stop words
        assert!(result.output.len() < input.len());
        assert!(result.compression_ratio < 1.0);
        // Content words should survive
        assert!(result.output.contains("quick") || result.output.contains("fox"));
    }

    #[test]
    fn test_high_compression() {
        let c = make_compressor();
        let settings = CompressionSettings {
            aggressiveness: 0.9,
            ..Default::default()
        };
        let input = "the quick brown fox jumps over the lazy dog and it was a very good day";
        let result = c.compress(input, &settings).unwrap();

        assert!(result.compression_ratio < 0.5, "high aggressiveness should compress heavily");
    }

    #[test]
    fn test_safe_tags() {
        let c = make_compressor();
        let settings = CompressionSettings {
            aggressiveness: 0.9,
            ..Default::default()
        };
        let input = "the <ttc_safe>critical value</ttc_safe> is important";
        let result = c.compress(input, &settings).unwrap();

        assert!(result.output.contains("critical"), "safe-tagged words must survive");
        assert!(result.output.contains("value"), "safe-tagged words must survive");
    }

    #[test]
    fn test_empty_input() {
        let c = make_compressor();
        let settings = CompressionSettings::default();
        let result = c.compress("", &settings);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_aggressiveness() {
        let c = make_compressor();
        let settings = CompressionSettings {
            aggressiveness: 1.5,
            ..Default::default()
        };
        let result = c.compress("hello", &settings);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_safe_regions() {
        let (cleaned, safe) = extract_safe_regions("hello <ttc_safe>world</ttc_safe> foo");
        assert_eq!(cleaned, "hello world foo");
        assert_eq!(safe, vec!["world"]);
    }

    #[test]
    fn test_multiple_safe_regions() {
        let (cleaned, safe) =
            extract_safe_regions("<ttc_safe>keep this</ttc_safe> and <ttc_safe>also this</ttc_safe>");
        assert_eq!(cleaned, "keep this and also this");
        assert_eq!(safe, vec!["keep", "this", "also", "this"]);
    }
}
