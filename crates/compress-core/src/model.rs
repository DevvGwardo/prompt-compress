use crate::error::{CompressError, Result};
use crate::scorer::{ScoredToken, TokenScorer};

/// ONNX-based scorer using a fine-tuned DistilBERT model.
///
/// Phase 3 implementation — currently a placeholder that falls back to heuristic scoring.
/// Once the training pipeline produces a model, this will load the ONNX file via `ort`
/// and perform real inference.
pub struct OnnxScorer {
    _model_path: String,
}

impl OnnxScorer {
    /// Load an ONNX model from the given path.
    ///
    /// Returns an error if the model file doesn't exist or can't be loaded.
    pub fn load(model_path: &str) -> Result<Self> {
        // Phase 3: actual ort::Session loading
        // let session = ort::Session::builder()?
        //     .with_optimization_level(ort::GraphOptimizationLevel::Level3)?
        //     .commit_from_file(model_path)?;

        if !std::path::Path::new(model_path).exists() {
            return Err(CompressError::Model(format!(
                "model file not found: {model_path}"
            )));
        }

        Ok(Self {
            _model_path: model_path.to_string(),
        })
    }
}

impl TokenScorer for OnnxScorer {
    fn score(&self, text: &str) -> Result<Vec<ScoredToken>> {
        // Phase 3: Real ONNX inference pipeline:
        // 1. Tokenize with HF tokenizer (WordPiece)
        // 2. Sliding window for long inputs (510 tokens, 75% overlap)
        // 3. Run ONNX inference on each window
        // 4. Aggregate sub-word scores to word-level (mean)
        // 5. Average overlapping window scores

        // Placeholder: use simple heuristic
        let scorer = crate::scorer::HeuristicScorer::new();
        scorer.score(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_not_found() {
        let result = OnnxScorer::load("/nonexistent/model.onnx");
        assert!(result.is_err());
    }
}
