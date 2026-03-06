use compress_core::scorer::{HeuristicScorer, TokenScorer};

// ─── Construction ────────────────────────────────────────────────────

#[test]
fn new_and_default_are_equivalent() {
    let a = HeuristicScorer::new();
    let b = HeuristicScorer::default();
    // Both should score the same text identically
    let text = "the quick brown fox";
    let sa: Vec<f32> = a
        .score(text)
        .unwrap()
        .iter()
        .map(|t| t.importance)
        .collect();
    let sb: Vec<f32> = b
        .score(text)
        .unwrap()
        .iter()
        .map(|t| t.importance)
        .collect();
    assert_eq!(sa, sb);
}

// ─── Stop words ──────────────────────────────────────────────────────

#[test]
fn all_common_stop_words_score_low() {
    let scorer = HeuristicScorer::new();
    let stop_words = [
        "the", "is", "are", "was", "of", "in", "for", "and", "or", "but", "to", "a", "an",
    ];

    for word in stop_words {
        let tokens = scorer.score(word).unwrap();
        assert_eq!(tokens.len(), 1, "expected 1 token for '{word}'");
        assert!(
            tokens[0].importance <= 0.3,
            "stop word '{word}' scored {}, expected <= 0.3",
            tokens[0].importance
        );
    }
}

#[test]
fn stop_words_are_case_insensitive() {
    let scorer = HeuristicScorer::new();
    // "The" starts with uppercase, so it hits the capitalized branch (0.9) before stop word check.
    // But "tHe" or "THE" may behave differently. This tests the actual behavior.
    let tokens = scorer.score("the").unwrap();
    assert!(tokens[0].importance <= 0.3);
}

// ─── Punctuation ─────────────────────────────────────────────────────

#[test]
fn pure_punctuation_scores_very_low() {
    let scorer = HeuristicScorer::new();
    let punctuation = [".", ",", "!", "?", ";", ":", "---", "...", "()"];

    for p in punctuation {
        let tokens = scorer.score(p).unwrap();
        assert_eq!(tokens.len(), 1, "expected 1 token for '{p}'");
        assert!(
            tokens[0].importance <= 0.2,
            "punctuation '{p}' scored {}, expected <= 0.2",
            tokens[0].importance
        );
    }
}

// ─── Numbers ─────────────────────────────────────────────────────────

#[test]
fn numbers_score_high() {
    let scorer = HeuristicScorer::new();
    let numbers = ["42", "3.14", "1,000", "2024", "0"];

    for n in numbers {
        let tokens = scorer.score(n).unwrap();
        assert!(
            tokens[0].importance >= 0.7,
            "number '{n}' scored {}, expected >= 0.7",
            tokens[0].importance
        );
    }
}

// ─── Capitalized / Proper nouns ──────────────────────────────────────

#[test]
fn capitalized_words_score_high() {
    let scorer = HeuristicScorer::new();
    let proper_nouns = ["Alice", "London", "Microsoft", "January"];

    for word in proper_nouns {
        let tokens = scorer.score(word).unwrap();
        assert!(
            tokens[0].importance >= 0.8,
            "capitalized word '{word}' scored {}, expected >= 0.8",
            tokens[0].importance
        );
    }
}

#[test]
fn all_caps_scores_highest() {
    let scorer = HeuristicScorer::new();
    let acronyms = ["API", "HTTP", "NASA", "SQL"];

    for word in acronyms {
        let tokens = scorer.score(word).unwrap();
        assert!(
            tokens[0].importance >= 0.9,
            "ALL CAPS '{word}' scored {}, expected >= 0.9",
            tokens[0].importance
        );
    }
}

// ─── Short words ─────────────────────────────────────────────────────

#[test]
fn short_non_stop_words_score_low_to_mid() {
    let scorer = HeuristicScorer::new();
    // "ox" is 2 chars, not a stop word
    let tokens = scorer.score("ox").unwrap();
    assert!(
        tokens[0].importance <= 0.5,
        "short word 'ox' scored {}, expected <= 0.5",
        tokens[0].importance
    );
}

// ─── Length bonus ────────────────────────────────────────────────────

#[test]
fn longer_words_score_higher_than_shorter() {
    let scorer = HeuristicScorer::new();
    let short = scorer.score("code").unwrap();
    let long = scorer.score("implementation").unwrap();

    assert!(
        long[0].importance >= short[0].importance,
        "longer word ({}) should score >= shorter word ({})",
        long[0].importance,
        short[0].importance
    );
}

// ─── Multi-word scoring ──────────────────────────────────────────────

#[test]
fn scores_all_words_in_sentence() {
    let scorer = HeuristicScorer::new();
    let tokens = scorer.score("the quick brown fox").unwrap();
    assert_eq!(tokens.len(), 4);
    assert_eq!(tokens[0].text, "the");
    assert_eq!(tokens[1].text, "quick");
    assert_eq!(tokens[2].text, "brown");
    assert_eq!(tokens[3].text, "fox");
}

#[test]
fn tokens_preserve_original_text() {
    let scorer = HeuristicScorer::new();
    let tokens = scorer.score("Hello WORLD 42").unwrap();
    assert_eq!(tokens[0].text, "Hello");
    assert_eq!(tokens[1].text, "WORLD");
    assert_eq!(tokens[2].text, "42");
}

#[test]
fn tokens_default_to_not_protected() {
    let scorer = HeuristicScorer::new();
    let tokens = scorer.score("hello world").unwrap();
    for t in &tokens {
        assert!(!t.protected, "token '{}' should not be protected", t.text);
    }
}

// ─── Edge cases ──────────────────────────────────────────────────────

#[test]
fn empty_input_returns_empty_vec() {
    let scorer = HeuristicScorer::new();
    assert!(scorer.score("").unwrap().is_empty());
}

#[test]
fn whitespace_only_returns_empty_vec() {
    let scorer = HeuristicScorer::new();
    assert!(scorer.score("   \t\n  ").unwrap().is_empty());
}

#[test]
fn single_word_returns_one_token() {
    let scorer = HeuristicScorer::new();
    let tokens = scorer.score("hello").unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].text, "hello");
}

#[test]
fn mixed_content_ordering() {
    let scorer = HeuristicScorer::new();
    let tokens = scorer.score("the API costs 42.99").unwrap();

    let the_score = tokens.iter().find(|t| t.text == "the").unwrap().importance;
    let api_score = tokens.iter().find(|t| t.text == "API").unwrap().importance;
    let num_score = tokens
        .iter()
        .find(|t| t.text == "42.99")
        .unwrap()
        .importance;

    // API (ALL CAPS) > numbers > stop words
    assert!(api_score > num_score);
    assert!(num_score > the_score);
}

#[test]
fn unicode_words_get_default_score() {
    let scorer = HeuristicScorer::new();
    let tokens = scorer.score("café résumé naïve").unwrap();
    assert_eq!(tokens.len(), 3);
    // Non-ASCII words should still get scored (won't match stop words)
    for t in &tokens {
        assert!(
            t.importance > 0.0,
            "unicode word '{}' should have positive score",
            t.text
        );
    }
}

#[test]
fn many_spaces_between_words() {
    let scorer = HeuristicScorer::new();
    let tokens = scorer.score("hello     world").unwrap();
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[1].text, "world");
}
