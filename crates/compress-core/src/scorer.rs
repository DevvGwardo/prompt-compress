use crate::error::Result;

/// A scored word with its importance.
#[derive(Debug, Clone)]
pub struct ScoredToken {
    /// The original word/token text.
    pub text: String,
    /// Importance score between 0.0 and 1.0.
    pub importance: f32,
    /// Whether this token is protected (inside `<ttc_safe>` tags).
    pub protected: bool,
}

/// Trait for scoring token importance.
pub trait TokenScorer: Send + Sync {
    /// Score each word in the input text.
    fn score(&self, text: &str) -> Result<Vec<ScoredToken>>;
}

/// Heuristic-based scorer using stop-word frequency and token characteristics.
pub struct HeuristicScorer {
    stop_words: Vec<&'static str>,
}

impl HeuristicScorer {
    pub fn new() -> Self {
        Self {
            stop_words: vec![
                "a", "an", "the", "is", "are", "was", "were", "be", "been", "being",
                "have", "has", "had", "do", "does", "did", "will", "would", "could",
                "should", "may", "might", "shall", "can", "need", "dare", "ought",
                "used", "to", "of", "in", "for", "on", "with", "at", "by", "from",
                "as", "into", "through", "during", "before", "after", "above", "below",
                "between", "out", "off", "over", "under", "again", "further", "then",
                "once", "here", "there", "when", "where", "why", "how", "all", "both",
                "each", "few", "more", "most", "other", "some", "such", "no", "nor",
                "not", "only", "own", "same", "so", "than", "too", "very", "just",
                "because", "but", "and", "or", "if", "while", "about", "up", "that",
                "this", "these", "those", "it", "its", "i", "me", "my", "we", "our",
                "you", "your", "he", "him", "his", "she", "her", "they", "them", "their",
                "what", "which", "who", "whom", "also", "still", "already", "yet",
            ],
        }
    }

    fn is_stop_word(&self, word: &str) -> bool {
        self.stop_words.contains(&word.to_lowercase().as_str())
    }

    /// Compute importance score for a single word.
    fn word_importance(&self, word: &str) -> f32 {
        let lower = word.to_lowercase();

        // Pure punctuation gets very low importance
        if word.chars().all(|c| c.is_ascii_punctuation()) {
            return 0.15;
        }

        // Numbers are moderately important (often data)
        if word.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',') {
            return 0.8;
        }

        // Stop words get low importance
        if self.is_stop_word(&lower) {
            return 0.2;
        }

        // Short words (1-2 chars) that aren't stop words
        if lower.len() <= 2 {
            return 0.3;
        }

        // Capitalized words (potential proper nouns, acronyms) are more important
        if word.chars().next().is_some_and(|c| c.is_uppercase()) {
            return 0.9;
        }

        // ALL CAPS (acronyms, emphasis) are very important
        if word.len() > 1 && word.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()) {
            return 0.95;
        }

        // Longer words tend to carry more semantic meaning
        let len_bonus = (lower.len() as f32 / 12.0).min(1.0) * 0.2;

        0.5 + len_bonus
    }
}

impl Default for HeuristicScorer {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenScorer for HeuristicScorer {
    fn score(&self, text: &str) -> Result<Vec<ScoredToken>> {
        let mut tokens = Vec::new();

        // Split into words preserving whitespace context
        for word in text.split_whitespace() {
            let importance = self.word_importance(word);
            tokens.push(ScoredToken {
                text: word.to_string(),
                importance,
                protected: false,
            });
        }

        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_words_score_low() {
        let scorer = HeuristicScorer::new();
        let tokens = scorer.score("the quick brown fox jumps over the lazy dog").unwrap();

        let the_score = tokens.iter().find(|t| t.text == "the").unwrap().importance;
        let fox_score = tokens.iter().find(|t| t.text == "fox").unwrap().importance;
        let over_score = tokens.iter().find(|t| t.text == "over").unwrap().importance;

        assert!(the_score < fox_score, "stop word 'the' should score lower than 'fox'");
        assert!(over_score < fox_score, "stop word 'over' should score lower than 'fox'");
    }

    #[test]
    fn test_capitalized_words_important() {
        let scorer = HeuristicScorer::new();
        let tokens = scorer.score("John went home").unwrap();

        let john_score = tokens.iter().find(|t| t.text == "John").unwrap().importance;
        let home_score = tokens.iter().find(|t| t.text == "home").unwrap().importance;

        assert!(john_score > home_score, "proper noun 'John' should score higher");
    }

    #[test]
    fn test_numbers_important() {
        let scorer = HeuristicScorer::new();
        let tokens = scorer.score("the price is 42.99").unwrap();

        let price_token = tokens.iter().find(|t| t.text == "42.99").unwrap();
        let the_token = tokens.iter().find(|t| t.text == "the").unwrap();

        assert!(price_token.importance > the_token.importance);
    }

    #[test]
    fn test_empty_input() {
        let scorer = HeuristicScorer::new();
        let tokens = scorer.score("").unwrap();
        assert!(tokens.is_empty());
    }
}
