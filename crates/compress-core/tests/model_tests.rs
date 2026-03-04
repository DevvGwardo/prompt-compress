#[test]
fn onnx_scorer_rejects_missing_model() {
    let result = compress_core::model::OnnxScorer::load(
        "/nonexistent/model.onnx",
        "/nonexistent/tokenizer.json"
    );
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(err.to_string().contains("not found"), "error: {err}");
}

#[test]
fn onnx_scorer_error_includes_path() {
    let model_path = "/some/fake/path/model.onnx";
    let tok_path = "/some/fake/path/tokenizer.json";
    let err = compress_core::model::OnnxScorer::load(model_path, tok_path)
        .err()
        .unwrap();
    assert!(
        err.to_string().contains(model_path),
        "error should include path: {err}"
    );
}

#[test]
fn onnx_scorer_with_real_file_but_not_onnx() {
    // Create temp files that exist but aren't valid ONNX/tokenizer files.
    // The OnnxScorer::load will attempt to load them with the ort library,
    // which should fail since they're not valid.
    let tmp_model = std::env::temp_dir().join("fake_model.onnx");
    let tmp_tokenizer = std::env::temp_dir().join("fake_tokenizer.json");
    std::fs::write(&tmp_model, b"not a real onnx model").unwrap();
    std::fs::write(&tmp_tokenizer, b"{}").unwrap(); // Minimal JSON

    let result = compress_core::model::OnnxScorer::load(
        tmp_model.to_str().unwrap(),
        tmp_tokenizer.to_str().unwrap()
    );
    // This will fail because the files aren't valid - tokenizer.json is empty
    // and model.onnx is not a valid ONNX file
    assert!(result.is_err());

    std::fs::remove_file(tmp_model).ok();
    std::fs::remove_file(tmp_tokenizer).ok();
}
