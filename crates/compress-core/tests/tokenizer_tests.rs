use compress_core::LlmTokenCounter;

#[test]
fn counts_simple_text() {
    let counter = LlmTokenCounter::new("gpt-4").unwrap();
    let count = counter.count("Hello, world!");
    assert!(count > 0 && count < 10);
}

#[test]
fn empty_string_is_zero_tokens() {
    let counter = LlmTokenCounter::new("gpt-4").unwrap();
    assert_eq!(counter.count(""), 0);
}

#[test]
fn whitespace_only_has_tokens() {
    let counter = LlmTokenCounter::new("gpt-4").unwrap();
    // Whitespace is encoded as tokens by BPE
    let count = counter.count("   ");
    assert!(count >= 1);
}

#[test]
fn longer_text_has_more_tokens() {
    let counter = LlmTokenCounter::new("gpt-4").unwrap();
    let short = counter.count("hello");
    let long = counter.count("hello world, this is a longer sentence with more words");
    assert!(long > short);
}

#[test]
fn single_word_has_at_least_one_token() {
    let counter = LlmTokenCounter::new("gpt-4").unwrap();
    assert!(counter.count("hello") >= 1);
}

#[test]
fn numbers_are_tokenized() {
    let counter = LlmTokenCounter::new("gpt-4").unwrap();
    assert!(counter.count("123456789") >= 1);
}

#[test]
fn unicode_text_is_tokenized() {
    let counter = LlmTokenCounter::new("gpt-4").unwrap();
    let count = counter.count("こんにちは世界");
    assert!(count >= 1);
}

#[test]
fn counter_is_reusable() {
    let counter = LlmTokenCounter::new("gpt-4").unwrap();
    let a = counter.count("first");
    let b = counter.count("second");
    assert!(a >= 1);
    assert!(b >= 1);
}

#[test]
fn different_model_names_still_work() {
    // Currently all models use cl100k_base, so any name should work.
    let counter = LlmTokenCounter::new("claude").unwrap();
    assert!(counter.count("hello") >= 1);

    let counter2 = LlmTokenCounter::new("gpt-3.5-turbo").unwrap();
    assert!(counter2.count("hello") >= 1);
}

#[test]
fn deterministic_counts() {
    let counter = LlmTokenCounter::new("gpt-4").unwrap();
    let text = "The quick brown fox jumps over the lazy dog";
    let c1 = counter.count(text);
    let c2 = counter.count(text);
    assert_eq!(c1, c2);
}

#[test]
fn known_token_count() {
    // "Hello world" should be ~2 tokens with cl100k_base
    let counter = LlmTokenCounter::new("gpt-4").unwrap();
    let count = counter.count("Hello world");
    assert_eq!(count, 2);
}

#[test]
fn special_characters() {
    let counter = LlmTokenCounter::new("gpt-4").unwrap();
    let count = counter.count("!@#$%^&*()");
    assert!(count >= 1);
}
