use compress_core::scorer::TokenScorer;

#[test]
fn onnx_scorer_rejects_missing_model() {
    let result = compress_core::model::OnnxScorer::load("/nonexistent/model.onnx");
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(err.to_string().contains("not found"), "error: {err}");
}

#[test]
fn onnx_scorer_error_includes_path() {
    let path = "/some/fake/path/model.onnx";
    let err = compress_core::model::OnnxScorer::load(path).err().unwrap();
    assert!(err.to_string().contains(path), "error should include path: {err}");
}

#[test]
fn onnx_scorer_with_real_file_but_not_onnx() {
    // Create a temp file that exists but isn't a valid ONNX model.
    // The current placeholder implementation only checks existence.
    let tmp = std::env::temp_dir().join("fake_model.onnx");
    std::fs::write(&tmp, b"not a real onnx model").unwrap();

    let result = compress_core::model::OnnxScorer::load(tmp.to_str().unwrap());
    // Currently succeeds because placeholder only checks file existence.
    assert!(result.is_ok());

    // Verify it can still score (falls back to heuristic).
    let scorer = result.unwrap();
    let tokens = scorer.score("hello world").unwrap();
    assert!(!tokens.is_empty());

    std::fs::remove_file(tmp).ok();
}
