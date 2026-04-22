use crate::error::Result;
use serde::{Deserialize, Serialize};

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

/// Scoring mode for heuristic token importance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeuristicMode {
    /// Standard general-purpose scoring.
    Standard,
    /// Agent-aware scoring optimized for function calls, code blocks, and instruction verb importance.
    AgentAware,
}

/// Common stop-word list for Standard mode.
const STOP_WORDS_STANDARD: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "been", "being", "have", "has",
    "had", "do", "does", "did", "will", "would", "could", "should", "may", "might",
    "shall", "can", "need", "dare", "ought", "used", "to", "of", "in", "for", "on",
    "with", "at", "by", "from", "as", "into", "through", "during", "before", "after",
    "above", "below", "between", "out", "off", "over", "under", "again", "further",
    "then", "once", "here", "there", "when", "where", "why", "how", "all", "both",
    "each", "few", "more", "most", "other", "some", "such", "no", "nor", "not", "only",
    "own", "same", "so", "than", "too", "very", "just", "because", "but", "and", "or",
    "if", "while", "about", "up", "that", "this", "these", "those", "it", "its", "i",
    "me", "my", "we", "our", "you", "your", "he", "him", "his", "she", "her", "they",
    "them", "their", "what", "which", "who", "whom", "also", "still", "already", "yet",
];

/// Stop-words for AgentAware mode — even more aggressively demoted conversational filler.
const STOP_WORDS_AGENT: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "been", "being", "have", "has",
    "had", "do", "does", "did", "will", "would", "could", "should", "may", "might",
    "shall", "can", "need", "dare", "ought", "used", "to", "of", "in", "for", "on",
    "with", "at", "by", "from", "as", "into", "through", "during", "before", "after",
    "above", "below", "between", "out", "off", "over", "under", "again", "further",
    "then", "once", "here", "there", "when", "where", "why", "how", "all", "both",
    "each", "few", "more", "most", "other", "some", "such", "no", "nor", "not", "only",
    "own", "same", "so", "than", "too", "very", "just", "because", "but", "and", "or",
    "if", "while", "about", "up", "that", "this", "these", "those", "it", "its", "i",
    "me", "my", "we", "our", "you", "your", "he", "him", "his", "she", "her", "they",
    "them", "their", "what", "which", "who", "whom", "also", "still", "already", "yet",
    // Additional conversational fillers when talking to/through an agent
    "please", "thank", "thanks", "could", "would", "can",
    "hey", "ok", "okay", "got", "sure", "alright",
];

/// Instruction verbs that are critical to preserve in agent prompts.
const INSTRUCTION_VERBS: &[&str] = &[
    "create", "analyze", "fix", "update", "delete", "add", "remove", "list", "get",
    "find", "search", "write", "read", "build", "deploy", "run", "execute", "call",
    "invoke", "fetch", "send", "generate", "extract", "validate", "verify", "test",
    "compile", "install", "configure", "setup", "init", "start", "stop", "restart",
    "debug", "trace", "monitor", "log", "print",
];

/// Heuristic-based scorer using stop-word frequency and token characteristics.
pub struct HeuristicScorer {
    mode: HeuristicMode,
    stop_words: Vec<&'static str>,
}

impl HeuristicScorer {
    /// Create a scorer in Standard mode (default).
    pub fn new() -> Self {
        Self::with_mode(HeuristicMode::Standard)
    }

    /// Create a scorer configured for the given mode.
    pub fn with_mode(mode: HeuristicMode) -> Self {
        let stop_words = match mode {
            HeuristicMode::Standard => STOP_WORDS_STANDARD.iter().copied().collect(),
            HeuristicMode::AgentAware => STOP_WORDS_AGENT.iter().copied().collect(),
        };
        Self { mode, stop_words }
    }

    fn is_stop_word(&self, word: &str) -> bool {
        self.stop_words.contains(&word.to_lowercase().as_str())
    }

    /// Compute importance score for a single word.
    fn word_importance(&self, word: &str) -> f32 {
        let lower = word.to_lowercase();
        let is_stop = self.is_stop_word(word);

        // Pure punctuation gets very low importance
        if word.chars().all(|c| c.is_ascii_punctuation()) {
            return 0.15;
        }

        // Numbers are moderately important (often data)
        if word
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
        {
            return 0.8;
        }

        // Stop words importance depends on mode
        if is_stop {
            return match self.mode {
                HeuristicMode::Standard => 0.2,
                HeuristicMode::AgentAware => 0.1, // demote filler even more for agent prompts
            };
        }

        // Short words (1-2 chars) that aren't stop words matter slightly
        if lower.len() <= 2 {
            return 0.3;
        }

        // Capitalized words (proper nouns, acronyms) are important
        if word.chars().next().is_some_and(|c| c.is_uppercase()) {
            return 0.9;
        }

        // ALL CAPS (acronyms, emphasis) are very important
        if word.len() > 1 && word.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()) {
            return 0.95;
        }

        // In AgentAware mode, boost instruction verbs heavily
        if self.mode == HeuristicMode::AgentAware {
            if INSTRUCTION_VERBS.contains(&lower.as_str()) {
                return 0.95;
            }
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
        let tokens = scorer
            .score("the quick brown fox jumps over the lazy dog")
            .unwrap();

        let the_score = tokens.iter().find(|t| t.text == "the").unwrap().importance;
        let fox_score = tokens.iter().find(|t| t.text == "fox").unwrap().importance;
        let over_score = tokens.iter().find(|t| t.text == "over").unwrap().importance;

        assert!(
            the_score < fox_score,
            "stop word 'the' should score lower than 'fox'"
        );
        assert!(
            over_score < fox_score,
            "stop word 'over' should score lower than 'fox'"
        );
    }

    #[test]
    fn test_capitalized_words_important() {
        let scorer = HeuristicScorer::new();
        let tokens = scorer.score("John went home").unwrap();

        let john_score = tokens.iter().find(|t| t.text == "John").unwrap().importance;
        let home_score = tokens.iter().find(|t| t.text == "home").unwrap().importance;

        assert!(
            john_score > home_score,
            "proper noun 'John' should score higher"
        );
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

    #[test]
    fn test_agent_aware_mode_instruction_verb_boost() {
        let scorer = HeuristicScorer::with_mode(HeuristicMode::AgentAware);
        let tokens = scorer.score("please create a new function and fix the bug").unwrap();

        let create_score = tokens.iter().find(|t| t.text == "create").unwrap().importance;
        let fix_score = tokens.iter().find(|t| t.text == "fix").unwrap().importance;
        let please_score = tokens.iter().find(|t| t.text == "please").unwrap().importance;

        assert!(
            create_score >= 0.9,
            "instruction verb 'create' should be very important"
        );
        assert!(
            fix_score >= 0.9,
            "instruction verb 'fix' should be very important"
        );
        assert!(
            please_score < create_score,
            "conversational filler 'please' should score lower than instruction verbs"
        );
    }

    #[test]
    fn test_standard_mode_instruction_verb() {
        let scorer = HeuristicScorer::new();
        let tokens = scorer.score("create a new function").unwrap();
        let create_score = tokens.iter().find(|t| t.text == "create").unwrap().importance;
        // In standard mode, "create" is not a stop word, so should get baseline~0.5+len_bonus
        assert!(create_score > 0.5, "create should be moderately important");
    }
}
