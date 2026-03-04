use compress_core::CompressError;

#[test]
fn tokenizer_error_display() {
    let err = CompressError::Tokenizer("bad encoding".to_string());
    assert_eq!(err.to_string(), "tokenizer error: bad encoding");
}

#[test]
fn model_error_display() {
    let err = CompressError::Model("file not found".to_string());
    assert_eq!(err.to_string(), "model error: file not found");
}

#[test]
fn invalid_aggressiveness_display() {
    let err = CompressError::InvalidAggressiveness(1.5);
    assert_eq!(
        err.to_string(),
        "invalid aggressiveness 1.5: must be between 0.0 and 1.0"
    );
}

#[test]
fn invalid_aggressiveness_negative_display() {
    let err = CompressError::InvalidAggressiveness(-0.1);
    assert!(err.to_string().contains("-0.1"));
    assert!(err.to_string().contains("aggressiveness"));
}

#[test]
fn empty_input_display() {
    let err = CompressError::EmptyInput;
    assert_eq!(err.to_string(), "input is empty");
}

#[test]
fn config_error_display() {
    let err = CompressError::Config("missing field".to_string());
    assert_eq!(err.to_string(), "config error: missing field");
}

#[test]
fn anyhow_error_converts_to_compress_error() {
    let anyhow_err = anyhow::anyhow!("something went wrong");
    let compress_err: CompressError = anyhow_err.into();
    assert!(compress_err.to_string().contains("something went wrong"));
}

#[test]
fn error_is_send_and_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<CompressError>();
    assert_sync::<CompressError>();
}

#[test]
fn error_is_debug() {
    let err = CompressError::EmptyInput;
    let debug_str = format!("{:?}", err);
    assert!(debug_str.contains("EmptyInput"));
}
