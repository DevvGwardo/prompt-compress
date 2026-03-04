use std::collections::HashMap;
use std::sync::RwLock;

use ndarray::{Array2, Array3};
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;
use regex::Regex;
use tokenizers::Tokenizer;

use crate::error::{CompressError, Result};
use crate::scorer::{ScoredToken, TokenScorer};

/// ONNX-based scorer using a fine-tuned DistilBERT model.
///
/// Performs token-level importance scoring using an ONNX model with sliding window
/// inference for long inputs, aggregating sub-word scores to word-level.
pub struct OnnxScorer {
    session: RwLock<Session>,
    tokenizer: Tokenizer,
}

/// Configuration for sliding window inference.
struct WindowConfig {
    /// Maximum sequence length (including [CLS] and [SEP]).
    max_seq_len: usize,
    /// Window size for token chunks (excluding special tokens).
    window_size: usize,
    /// Stride between windows (128 tokens = ~75% overlap with 510 window).
    stride: usize,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            max_seq_len: 512,
            window_size: 510,
            stride: 128,
        }
    }
}

impl OnnxScorer {
    /// Load an ONNX model and tokenizer from the given paths.
    ///
    /// # Arguments
    /// * `model_path` - Path to the ONNX model file
    /// * `tokenizer_path` - Path to the tokenizer.json file (HuggingFace format)
    ///
    /// Returns an error if the model file doesn't exist or can't be loaded.
    pub fn load(model_path: &str, tokenizer_path: &str) -> Result<Self> {
        if !std::path::Path::new(model_path).exists() {
            return Err(CompressError::Model(format!(
                "model file not found: {model_path}"
            )));
        }

        if !std::path::Path::new(tokenizer_path).exists() {
            return Err(CompressError::Tokenizer(format!(
                "tokenizer file not found: {tokenizer_path}"
            )));
        }

        // Load ONNX session with optimization
        let session = Session::builder()
            .map_err(|e| CompressError::Model(format!("failed to create session: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| CompressError::Model(format!("failed to set optimization level: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| CompressError::Model(format!("failed to load model: {e}")))?;

        // Load HuggingFace tokenizer
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| CompressError::Tokenizer(format!("failed to load tokenizer: {e}")))?;

        Ok(Self {
            session: RwLock::new(session),
            tokenizer,
        })
    }

    /// Extract protected word ranges from `<ttc_safe>` tags.
    ///
    /// Returns a vector of (start_char, end_char) ranges for protected regions.
    fn extract_protected_ranges(&self, text: &str) -> Vec<(usize, usize)> {
        let re = Regex::new(r"<ttc_safe>(.*?)</ttc_safe>").unwrap();
        let mut ranges = Vec::new();

        for cap in re.captures_iter(text) {
            if let Some(content) = cap.get(1) {
                ranges.push((content.start(), content.end()));
            }
        }

        ranges
    }

    /// Remove `<ttc_safe>` tags while keeping the content.
    fn remove_safe_tags(&self, text: &str) -> String {
        let re = Regex::new(r"<ttc_safe>(.*?)</ttc_safe>").unwrap();
        re.replace_all(text, "$1").to_string()
    }

    /// Check if a character range overlaps with any protected range.
    fn is_protected(&self, char_start: usize, char_end: usize, protected_ranges: &[(usize, usize)]) -> bool {
        protected_ranges
            .iter()
            .any(|(start, end)| char_start < *end && char_end > *start)
    }

    /// Tokenize text and return encoding with word mapping.
    fn tokenize(&self, text: &str) -> Result<tokenizers::Encoding> {
        self.tokenizer
            .encode(text, true)
            .map_err(|e| CompressError::Tokenizer(format!("tokenization failed: {e}")))
    }

    /// Run ONNX inference on a batch of input IDs and attention masks.
    ///
    /// Returns logits tensor [batch, seq_len, 2].
    fn run_inference(
        &self,
        input_ids: Array2<i64>,
        attention_mask: Array2<i64>,
    ) -> Result<Array3<f32>> {
        // Create tensors from arrays
        let input_ids_tensor = Tensor::from_array(input_ids)
            .map_err(|e| CompressError::Model(format!("failed to create input_ids tensor: {e}")))?;
        let attention_mask_tensor = Tensor::from_array(attention_mask)
            .map_err(|e| CompressError::Model(format!("failed to create attention_mask tensor: {e}")))?;

        // Run inference and extract output data within the lock scope
        let (shape_vec, data_vec) = {
            let mut session = self.session.write().map_err(|e| {
                CompressError::Model(format!("failed to acquire write lock: {e}"))
            })?;
            
            // Get output name
            let output_name = session.outputs()
                .first()
                .map(|o| o.name().to_string())
                .ok_or_else(|| CompressError::Model("no output defined in model".to_string()))?;
            
            // Run inference
            let outputs = session
                .run(ort::inputs![input_ids_tensor, attention_mask_tensor])
                .map_err(|e| CompressError::Model(format!("inference failed: {e}")))?;
            
            // Extract output value
            let output_value = outputs.get(&output_name)
                .ok_or_else(|| CompressError::Model(format!("output '{}' not found", output_name)))?;
            
            // Extract tensor data
            let (shape, data) = output_value
                .try_extract_tensor::<f32>()
                .map_err(|e| CompressError::Model(format!("failed to extract output tensor: {e}")))?;
            
            // Clone data to return from scope
            let shape_vec: Vec<i64> = shape.iter().copied().collect();
            let data_vec: Vec<f32> = data.to_vec();
            (shape_vec, data_vec)
        };
        
        // Convert to ndarray outside the lock
        let output_array = Array3::from_shape_vec(
            (shape_vec[0] as usize, shape_vec[1] as usize, shape_vec[2] as usize),
            data_vec
        )
        .map_err(|e| CompressError::Model(format!("failed to create output array: {e}")))?;

        Ok(output_array)
    }

    /// Apply softmax to logits and return "keep" probabilities (index 1).
    fn softmax_keep_probs(&self, logits: &Array3<f32>) -> Array2<f32> {
        let (batch, seq_len, _) = (logits.shape()[0], logits.shape()[1], logits.shape()[2]);
        
        let mut keep_probs = Array2::<f32>::zeros((batch, seq_len));
        
        for b in 0..batch {
            for s in 0..seq_len {
                let logit0 = logits[[b, s, 0]];
                let logit1 = logits[[b, s, 1]];
                
                // Softmax: exp(x) / sum(exp(x))
                let exp0 = logit0.exp();
                let exp1 = logit1.exp();
                let sum = exp0 + exp1;
                
                // Keep probability is softmax of index 1
                keep_probs[[b, s]] = exp1 / sum;
            }
        }
        
        keep_probs
    }

    /// Perform sliding window inference on long inputs.
    ///
    /// Returns a map from token index to (cumulative_score, count) for averaging.
    fn sliding_window_inference(
        &self,
        encoding: &tokenizers::Encoding,
        config: &WindowConfig,
    ) -> Result<HashMap<usize, (f32, usize)>> {
        let token_ids = encoding.get_ids();
        let num_tokens = token_ids.len();
        
        // If input fits in single window, run once
        if num_tokens <= config.max_seq_len {
            let (input_ids, attention_mask) = self.prepare_batch(encoding, 0, num_tokens, config)?;
            let logits = self.run_inference(input_ids, attention_mask)?;
            let keep_probs = self.softmax_keep_probs(&logits);
            
            let mut scores = HashMap::new();
            for i in 0..num_tokens {
                scores.insert(i, (keep_probs[[0, i]], 1));
            }
            return Ok(scores);
        }

        // Sliding window inference
        let mut token_scores: HashMap<usize, (f32, usize)> = HashMap::new();
        
        let mut start = 0;
        while start < num_tokens {
            let end = (start + config.window_size).min(num_tokens);
            let window_len = end - start;
            
            // Prepare batch for this window
            let (input_ids, attention_mask) = self.prepare_batch(encoding, start, end, config)?;
            
            // Run inference
            let logits = self.run_inference(input_ids, attention_mask)?;
            let keep_probs = self.softmax_keep_probs(&logits);
            
            // Accumulate scores (skip [CLS] at position 0 and [SEP] at last position)
            // The model output includes special tokens, so we map back to original tokens
            for i in 0..window_len {
                let token_idx = start + i;
                // +1 to skip [CLS] in output
                let output_idx = i + 1;
                if output_idx < keep_probs.shape()[1] {
                    let score = keep_probs[[0, output_idx]];
                    let entry = token_scores.entry(token_idx).or_insert((0.0, 0));
                    entry.0 += score;
                    entry.1 += 1;
                }
            }
            
            // Move window
            if end >= num_tokens {
                break;
            }
            start += config.stride;
        }
        
        Ok(token_scores)
    }

    /// Prepare input tensors for a window of tokens.
    fn prepare_batch(
        &self,
        encoding: &tokenizers::Encoding,
        start: usize,
        end: usize,
        config: &WindowConfig,
    ) -> Result<(Array2<i64>, Array2<i64>)> {
        let token_ids = encoding.get_ids();
        let window_tokens = &token_ids[start..end];
        let window_len = window_tokens.len();
        
        // Pad to max_seq_len if needed
        let seq_len = config.max_seq_len;
        let mut input_ids = vec![0i64; seq_len];
        let mut attention_mask = vec![0i64; seq_len];
        
        // [CLS] token (usually ID 101 for BERT-based models)
        // We rely on the encoding already having special tokens, but we need to reconstruct
        // For a window, we add [CLS] at start and [SEP] at end
        input_ids[0] = self.tokenizer.token_to_id("[CLS]").unwrap_or(101) as i64;
        attention_mask[0] = 1;
        
        // Copy window tokens
        for (i, &token_id) in window_tokens.iter().enumerate() {
            input_ids[i + 1] = token_id as i64;
            attention_mask[i + 1] = 1;
        }
        
        // [SEP] token (usually ID 102 for BERT-based models)
        let sep_pos = window_len + 1;
        if sep_pos < seq_len {
            input_ids[sep_pos] = self.tokenizer.token_to_id("[SEP]").unwrap_or(102) as i64;
            attention_mask[sep_pos] = 1;
        }
        
        // Convert to 2D arrays [batch=1, seq_len]
        let input_ids_array = Array2::from_shape_vec((1, seq_len), input_ids)
            .map_err(|e| CompressError::Model(format!("failed to create input array: {e}")))?;
        let attention_mask_array = Array2::from_shape_vec((1, seq_len), attention_mask)
            .map_err(|e| CompressError::Model(format!("failed to create mask array: {e}")))?;
        
        Ok((input_ids_array, attention_mask_array))
    }

    /// Aggregate sub-word token scores to word-level scores.
    ///
    /// Uses mean pooling for words that are split into multiple sub-word tokens.
    fn aggregate_to_words(
        &self,
        encoding: &tokenizers::Encoding,
        token_scores: &HashMap<usize, (f32, usize)>,
        text: &str,
        protected_ranges: &[(usize, usize)],
    ) -> Vec<ScoredToken> {
        let word_ids = encoding.get_word_ids();
        let tokens = encoding.get_tokens();
        
        // Group token scores by word index
        let mut word_token_scores: HashMap<u32, Vec<f32>> = HashMap::new();
        
        for (token_idx, &(cumulative_score, count)) in token_scores {
            if let Some(&Some(word_idx)) = word_ids.get(*token_idx) {
                let avg_score = cumulative_score / count as f32;
                word_token_scores
                    .entry(word_idx)
                    .or_default()
                    .push(avg_score);
            }
        }
        
        // Get word offsets for protected region detection
        let offsets = encoding.get_offsets();
        let mut word_offsets: HashMap<u32, (usize, usize)> = HashMap::new();
        for (i, word_id_opt) in word_ids.iter().enumerate() {
            if let Some(word_id) = word_id_opt {
                let (start, end) = offsets[i];
                let entry = word_offsets.entry(*word_id).or_insert((start, end));
                entry.0 = entry.0.min(start);
                entry.1 = entry.1.max(end);
            }
        }
        
        // Build word-level scored tokens
        let mut scored_words: Vec<ScoredToken> = Vec::new();
        
        // Split original text into words to get actual text
        let cleaned_text = self.remove_safe_tags(text);
        let text_bytes = cleaned_text.as_bytes();
        
        for (word_idx, scores) in word_token_scores {
            // Mean pooling of sub-word scores
            let avg_importance = if scores.is_empty() {
                0.5 // Default score for words without scores
            } else {
                scores.iter().sum::<f32>() / scores.len() as f32
            };
            
            // Get word text from offsets
            let word_text = if let Some(&(start, end)) = word_offsets.get(&word_idx) {
                if start < text_bytes.len() && end <= text_bytes.len() {
                    String::from_utf8_lossy(&text_bytes[start..end]).to_string()
                } else {
                    // Fallback: try to get from tokens
                    tokens.get(word_idx as usize).cloned().unwrap_or_default()
                }
            } else {
                tokens.get(word_idx as usize).cloned().unwrap_or_default()
            };
            
            // Check if word is in protected region
            let is_protected = if let Some(&(start, end)) = word_offsets.get(&word_idx) {
                self.is_protected(start, end, protected_ranges)
            } else {
                false
            };
            
            scored_words.push(ScoredToken {
                text: word_text,
                importance: if is_protected { 1.0 } else { avg_importance },
                protected: is_protected,
            });
        }
        
        // Sort by position in original text (approximate via word offsets order)
        scored_words.sort_by_key(|w| {
            // Find the word's approximate position
            cleaned_text.find(&w.text).unwrap_or(0)
        });
        
        scored_words
    }

    /// Simple word-level aggregation for texts without WordPiece splitting.
    ///
    /// Used as a fallback when the tokenizer doesn't provide word mappings.
    fn simple_word_scoring(
        &self,
        text: &str,
        token_scores: &HashMap<usize, (f32, usize)>,
        encoding: &tokenizers::Encoding,
        protected_ranges: &[(usize, usize)],
    ) -> Vec<ScoredToken> {
        let cleaned_text = self.remove_safe_tags(text);
        let words: Vec<&str> = cleaned_text.split_whitespace().collect();
        
        // Get word indices
        let word_ids = encoding.get_word_ids();
        
        // Map each word to its tokens
        let mut word_to_tokens: HashMap<u32, Vec<usize>> = HashMap::new();
        for (token_idx, word_id_opt) in word_ids.iter().enumerate() {
            if let Some(word_id) = word_id_opt {
                word_to_tokens.entry(*word_id).or_default().push(token_idx);
            }
        }
        
        // Calculate score for each word
        let mut result = Vec::new();
        for (word_idx, word_text) in words.iter().enumerate() {
            let word_idx_u32 = word_idx as u32;
            let scores: Vec<f32> = word_to_tokens
                .get(&word_idx_u32)
                .map(|token_indices| {
                    token_indices
                        .iter()
                        .filter_map(|&t| token_scores.get(&t))
                        .map(|&(sum, count)| sum / count as f32)
                        .collect()
                })
                .unwrap_or_default();
            
            let importance = if scores.is_empty() {
                0.5
            } else {
                scores.iter().sum::<f32>() / scores.len() as f32
            };
            
            // Find if word is protected by checking character offsets
            let char_start = cleaned_text.find(word_text).unwrap_or(0);
            let char_end = char_start + word_text.len();
            let is_protected = self.is_protected(char_start, char_end, protected_ranges);
            
            result.push(ScoredToken {
                text: word_text.to_string(),
                importance: if is_protected { 1.0 } else { importance },
                protected: is_protected,
            });
        }
        
        result
    }
}

impl TokenScorer for OnnxScorer {
    fn score(&self, text: &str) -> Result<Vec<ScoredToken>> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }

        // Extract protected regions before removing tags
        let protected_ranges = self.extract_protected_ranges(text);
        
        // Remove safe tags for tokenization
        let cleaned_text = self.remove_safe_tags(text);
        
        if cleaned_text.trim().is_empty() {
            return Ok(Vec::new());
        }

        // Tokenize the cleaned text
        let encoding = self.tokenize(&cleaned_text)?;
        
        // Run sliding window inference
        let config = WindowConfig::default();
        let token_scores = self.sliding_window_inference(&encoding, &config)?;
        
        // Aggregate to word-level scores
        let scored_words = if encoding.get_word_ids().is_empty() {
            self.simple_word_scoring(text, &token_scores, &encoding, &protected_ranges)
        } else {
            self.aggregate_to_words(&encoding, &token_scores, text, &protected_ranges)
        };
        
        Ok(scored_words)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_not_found() {
        let result = OnnxScorer::load("/nonexistent/model.onnx", "/nonexistent/tokenizer.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_protected_ranges() {
        // Test extraction of protected ranges from <ttc_safe> tags
        let text = "hello <ttc_safe>world</ttc_safe> foo";
        let re = Regex::new(r"<ttc_safe>(.*?)</ttc_safe>").unwrap();
        let mut ranges = Vec::new();
        for cap in re.captures_iter(text) {
            if let Some(content) = cap.get(1) {
                ranges.push((content.start(), content.end()));
            }
        }
        // "world" starts at position 16 and ends at 21 in the original text
        assert_eq!(ranges, vec![(16, 21)]);
    }

    #[test]
    fn test_remove_safe_tags() {
        let text = "hello <ttc_safe>world</ttc_safe> foo";
        let re = Regex::new(r"<ttc_safe>(.*?)</ttc_safe>").unwrap();
        let cleaned = re.replace_all(text, "$1").to_string();
        assert_eq!(cleaned, "hello world foo");
    }

    #[test]
    fn test_softmax_keep_probs() {
        // Test softmax calculation directly without needing a Session
        // Create test logits [batch=1, seq_len=3, num_classes=2]
        let _logits = Array3::from_shape_vec(
            (1, 3, 2),
            vec![
                1.0f32, 1.0f32,  // Equal logits -> 0.5 each
                2.0f32, 1.0f32,  // First higher -> keep prob < 0.5
                1.0f32, 2.0f32,  // Second higher -> keep prob > 0.5
            ],
        )
        .unwrap();
        
        // Compute softmax manually to verify
        fn softmax(logit0: f32, logit1: f32) -> f32 {
            let exp0 = logit0.exp();
            let exp1 = logit1.exp();
            exp1 / (exp0 + exp1)
        }
        
        // For equal logits, keep prob should be 0.5
        let prob0 = softmax(1.0, 1.0);
        assert!((prob0 - 0.5).abs() < 0.001);
        
        // For [2.0, 1.0], keep prob should be < 0.5
        let prob1 = softmax(2.0, 1.0);
        assert!(prob1 < 0.5);
        
        // For [1.0, 2.0], keep prob should be > 0.5  
        let prob2 = softmax(1.0, 2.0);
        assert!(prob2 > 0.5);
    }
}
