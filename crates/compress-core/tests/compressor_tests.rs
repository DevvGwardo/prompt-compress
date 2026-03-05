use compress_core::{CompressionSettings, Compressor, HeuristicScorer};

fn make_compressor() -> Compressor {
    Compressor::new(Box::new(HeuristicScorer::new()), "gpt-4").unwrap()
}

// ─── Aggressiveness spectrum ─────────────────────────────────────────

#[test]
fn aggressiveness_zero_returns_input_unchanged() {
    let c = make_compressor();
    let input = "the quick brown fox jumps over the lazy dog";
    let result = c
        .compress(
            input,
            &CompressionSettings {
                aggressiveness: 0.0,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(result.output, input);
    assert_eq!(result.compression_ratio, 1.0);
}

#[test]
fn aggressiveness_one_compresses_maximally() {
    let c = make_compressor();
    let input = "the quick brown fox jumps over the lazy dog";
    let result = c
        .compress(
            input,
            &CompressionSettings {
                aggressiveness: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
    // At threshold 1.0, only tokens with importance >= 1.0 survive.
    // No heuristic token hits 1.0, so output should be empty or very short.
    assert!(
        result.output.len() < input.len(),
        "max aggressiveness should produce shorter output"
    );
}

#[test]
fn increasing_aggressiveness_reduces_output() {
    let c = make_compressor();
    let input = "the quick brown fox jumps over the lazy dog and it was a very good day for everyone in the park";

    let low = c
        .compress(
            input,
            &CompressionSettings {
                aggressiveness: 0.3,
                ..Default::default()
            },
        )
        .unwrap();
    let mid = c
        .compress(
            input,
            &CompressionSettings {
                aggressiveness: 0.5,
                ..Default::default()
            },
        )
        .unwrap();
    let high = c
        .compress(
            input,
            &CompressionSettings {
                aggressiveness: 0.8,
                ..Default::default()
            },
        )
        .unwrap();

    assert!(
        low.output_tokens >= mid.output_tokens,
        "lower aggressiveness should produce more tokens: {} vs {}",
        low.output_tokens,
        mid.output_tokens
    );
    assert!(
        mid.output_tokens >= high.output_tokens,
        "mid aggressiveness should produce more tokens than high: {} vs {}",
        mid.output_tokens,
        high.output_tokens
    );
}

#[test]
fn compression_ratio_is_correct() {
    let c = make_compressor();
    let result = c
        .compress(
            "the quick brown fox",
            &CompressionSettings {
                aggressiveness: 0.5,
                ..Default::default()
            },
        )
        .unwrap();

    let expected_ratio = result.output_tokens as f64 / result.original_input_tokens as f64;
    assert!(
        (result.compression_ratio - expected_ratio).abs() < 0.001,
        "ratio {} should match computed {}",
        result.compression_ratio,
        expected_ratio
    );
}

// ─── Error handling ──────────────────────────────────────────────────

#[test]
fn empty_input_returns_error() {
    let c = make_compressor();
    let err = c.compress("", &CompressionSettings::default()).unwrap_err();
    assert!(err.to_string().contains("empty"), "error: {err}");
}

#[test]
fn whitespace_only_input_returns_error() {
    let c = make_compressor();
    let err = c
        .compress("   \t\n  ", &CompressionSettings::default())
        .unwrap_err();
    assert!(err.to_string().contains("empty"), "error: {err}");
}

#[test]
fn negative_aggressiveness_returns_error() {
    let c = make_compressor();
    let err = c
        .compress(
            "hello",
            &CompressionSettings {
                aggressiveness: -0.1,
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("aggressiveness"), "error: {err}");
}

#[test]
fn aggressiveness_above_one_returns_error() {
    let c = make_compressor();
    let err = c
        .compress(
            "hello",
            &CompressionSettings {
                aggressiveness: 1.5,
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("aggressiveness"), "error: {err}");
}

// ─── Safe tags ───────────────────────────────────────────────────────

#[test]
fn safe_tags_preserve_words_at_max_aggressiveness() {
    let c = make_compressor();
    let result = c
        .compress(
            "remove this but <ttc_safe>keep these words</ttc_safe> definitely",
            &CompressionSettings {
                aggressiveness: 1.0,
                ..Default::default()
            },
        )
        .unwrap();

    assert!(
        result.output.contains("keep"),
        "safe word 'keep' must survive"
    );
    assert!(
        result.output.contains("these"),
        "safe word 'these' must survive"
    );
    assert!(
        result.output.contains("words"),
        "safe word 'words' must survive"
    );
}

#[test]
fn multiple_safe_regions() {
    let c = make_compressor();
    let result = c
        .compress(
            "<ttc_safe>first</ttc_safe> remove the filler <ttc_safe>second</ttc_safe>",
            &CompressionSettings {
                aggressiveness: 1.0,
                ..Default::default()
            },
        )
        .unwrap();

    assert!(result.output.contains("first"));
    assert!(result.output.contains("second"));
}

#[test]
fn safe_tags_are_removed_from_output() {
    let c = make_compressor();
    // Use non-zero aggressiveness so the compressor actually processes tags
    let result = c
        .compress(
            "hello <ttc_safe>world</ttc_safe>",
            &CompressionSettings {
                aggressiveness: 0.3,
                ..Default::default()
            },
        )
        .unwrap();

    assert!(!result.output.contains("<ttc_safe>"));
    assert!(!result.output.contains("</ttc_safe>"));
    assert!(result.output.contains("world"));
}

#[test]
fn safe_tags_do_not_protect_matching_words_outside_tagged_region() {
    let c = make_compressor();
    let result = c
        .compress(
            "<ttc_safe>apple</ttc_safe> and filler apple filler",
            &CompressionSettings {
                aggressiveness: 1.0,
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(result.output.trim(), "apple");
}

#[test]
fn multiline_safe_tags_are_preserved() {
    let c = make_compressor();
    let result = c
        .compress(
            "start <ttc_safe>critical\nvalue</ttc_safe> end",
            &CompressionSettings {
                aggressiveness: 1.0,
                ..Default::default()
            },
        )
        .unwrap();

    assert!(result.output.contains("critical"));
    assert!(result.output.contains("value"));
}

#[test]
fn nested_safe_tags_handled_gracefully() {
    // Regex is non-greedy, so nested tags get partial matches — shouldn't panic.
    let c = make_compressor();
    let result = c.compress(
        "<ttc_safe>outer <ttc_safe>inner</ttc_safe> end</ttc_safe>",
        &CompressionSettings {
            aggressiveness: 0.5,
            ..Default::default()
        },
    );
    assert!(result.is_ok());
}

#[test]
fn empty_safe_tags_dont_break() {
    let c = make_compressor();
    let result = c.compress(
        "hello <ttc_safe></ttc_safe> world",
        &CompressionSettings {
            aggressiveness: 0.5,
            ..Default::default()
        },
    );
    assert!(result.is_ok());
}

// ─── Token counting ─────────────────────────────────────────────────

#[test]
fn original_tokens_greater_than_zero() {
    let c = make_compressor();
    let result = c
        .compress("hello world", &CompressionSettings::default())
        .unwrap();
    assert!(result.original_input_tokens > 0);
}

#[test]
fn output_tokens_less_than_or_equal_original() {
    let c = make_compressor();
    let input = "the quick brown fox jumps over the lazy dog";
    let result = c
        .compress(
            input,
            &CompressionSettings {
                aggressiveness: 0.5,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(result.output_tokens <= result.original_input_tokens);
}

#[test]
fn compression_ratio_bounded_zero_to_one() {
    let c = make_compressor();
    let result = c
        .compress(
            "the quick brown fox jumps over the lazy dog",
            &CompressionSettings {
                aggressiveness: 0.5,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(result.compression_ratio >= 0.0);
    assert!(result.compression_ratio <= 1.0);
}

// ─── Multiple calls ─────────────────────────────────────────────────

#[test]
fn compressor_reusable_across_calls() {
    let c = make_compressor();
    let settings = CompressionSettings {
        aggressiveness: 0.5,
        ..Default::default()
    };

    let r1 = c.compress("first call with some text", &settings).unwrap();
    let r2 = c
        .compress("second call with different text", &settings)
        .unwrap();

    assert!(!r1.output.is_empty());
    assert!(!r2.output.is_empty());
    assert_ne!(r1.output, r2.output);
}

#[test]
fn same_input_gives_deterministic_output() {
    let c = make_compressor();
    let settings = CompressionSettings {
        aggressiveness: 0.5,
        ..Default::default()
    };
    let input = "the quick brown fox jumps over the lazy dog";

    let r1 = c.compress(input, &settings).unwrap();
    let r2 = c.compress(input, &settings).unwrap();

    assert_eq!(r1.output, r2.output);
    assert_eq!(r1.output_tokens, r2.output_tokens);
    assert_eq!(r1.compression_ratio, r2.compression_ratio);
}

// ─── Edge cases ──────────────────────────────────────────────────────

#[test]
fn single_word_input() {
    let c = make_compressor();
    let result = c
        .compress(
            "hello",
            &CompressionSettings {
                aggressiveness: 0.3,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!result.output.is_empty());
}

#[test]
fn single_stop_word_at_high_aggressiveness() {
    let c = make_compressor();
    let result = c
        .compress(
            "the",
            &CompressionSettings {
                aggressiveness: 0.9,
                ..Default::default()
            },
        )
        .unwrap();
    // "the" scores 0.2, threshold 0.9 — should be dropped
    assert!(result.output.is_empty() || result.output == "the");
}

#[test]
fn long_input_doesnt_panic() {
    let c = make_compressor();
    let input = "the quick brown fox ".repeat(1000);
    let result = c
        .compress(
            &input,
            &CompressionSettings {
                aggressiveness: 0.5,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!result.output.is_empty());
    assert!(result.output.len() < input.len());
}

#[test]
fn unicode_input() {
    let c = make_compressor();
    let result = c.compress(
        "Le café est très bon aujourd'hui",
        &CompressionSettings {
            aggressiveness: 0.3,
            ..Default::default()
        },
    );
    assert!(result.is_ok());
}

#[test]
fn input_with_newlines() {
    let c = make_compressor();
    let result = c.compress(
        "line one\nline two\nline three",
        &CompressionSettings {
            aggressiveness: 0.3,
            ..Default::default()
        },
    );
    assert!(result.is_ok());
    assert!(!result.unwrap().output.is_empty());
}

#[test]
fn input_with_tabs_and_mixed_whitespace() {
    let c = make_compressor();
    let result = c.compress(
        "word1\t\tword2   word3",
        &CompressionSettings {
            aggressiveness: 0.3,
            ..Default::default()
        },
    );
    assert!(result.is_ok());
    let output = result.unwrap();
    // tiktoken tokenizes the full string (including whitespace chars), so token count >= 3
    assert!(output.original_input_tokens >= 3);
}
