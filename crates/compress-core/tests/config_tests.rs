use compress_core::{CompressionResult, CompressionSettings};

// ─── Default values ──────────────────────────────────────────────────

#[test]
fn default_aggressiveness_is_half() {
    let settings = CompressionSettings::default();
    assert_eq!(settings.aggressiveness, 0.5);
}

#[test]
fn default_target_model_is_gpt4() {
    let settings = CompressionSettings::default();
    assert_eq!(settings.target_model, "gpt-4");
}

// ─── Serialization ───────────────────────────────────────────────────

#[test]
fn settings_serializes_to_json() {
    let settings = CompressionSettings {
        aggressiveness: 0.5,
        target_model: "claude".to_string(),
    };
    let json = serde_json::to_value(&settings).unwrap();
    assert_eq!(json["aggressiveness"], 0.5); // 0.5 is exact in f32
    assert_eq!(json["target_model"], "claude");
}

#[test]
fn settings_deserializes_from_json() {
    let json = r#"{"aggressiveness": 0.3, "target_model": "gpt-3.5-turbo"}"#;
    let settings: CompressionSettings = serde_json::from_str(json).unwrap();
    assert_eq!(settings.aggressiveness, 0.3);
    assert_eq!(settings.target_model, "gpt-3.5-turbo");
}

#[test]
fn settings_deserializes_with_defaults() {
    let json = r#"{}"#;
    let settings: CompressionSettings = serde_json::from_str(json).unwrap();
    assert_eq!(settings.aggressiveness, 0.5);
    assert_eq!(settings.target_model, "gpt-4");
}

#[test]
fn settings_partial_json_uses_defaults() {
    let json = r#"{"aggressiveness": 0.9}"#;
    let settings: CompressionSettings = serde_json::from_str(json).unwrap();
    assert_eq!(settings.aggressiveness, 0.9);
    assert_eq!(settings.target_model, "gpt-4"); // default
}

// ─── CompressionResult ──────────────────────────────────────────────

#[test]
fn result_serializes_to_json() {
    let result = CompressionResult {
        output: "hello world".to_string(),
        output_tokens: 2,
        original_input_tokens: 5,
        compression_ratio: 0.4,
    };
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["output"], "hello world");
    assert_eq!(json["output_tokens"], 2);
    assert_eq!(json["original_input_tokens"], 5);
    assert_eq!(json["compression_ratio"], 0.4);
}

#[test]
fn result_roundtrips_through_json() {
    let original = CompressionResult {
        output: "compressed text".to_string(),
        output_tokens: 3,
        original_input_tokens: 10,
        compression_ratio: 0.3,
    };
    let json = serde_json::to_string(&original).unwrap();
    let deserialized: CompressionResult = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.output, original.output);
    assert_eq!(deserialized.output_tokens, original.output_tokens);
    assert_eq!(
        deserialized.original_input_tokens,
        original.original_input_tokens
    );
    assert_eq!(deserialized.compression_ratio, original.compression_ratio);
}

// ─── Clone ───────────────────────────────────────────────────────────

#[test]
fn settings_is_cloneable() {
    let settings = CompressionSettings {
        aggressiveness: 0.8,
        target_model: "test".to_string(),
    };
    let cloned = settings.clone();
    assert_eq!(cloned.aggressiveness, 0.8);
    assert_eq!(cloned.target_model, "test");
}

#[test]
fn result_is_cloneable() {
    let result = CompressionResult {
        output: "test".to_string(),
        output_tokens: 1,
        original_input_tokens: 5,
        compression_ratio: 0.2,
    };
    let cloned = result.clone();
    assert_eq!(cloned.output, "test");
}
