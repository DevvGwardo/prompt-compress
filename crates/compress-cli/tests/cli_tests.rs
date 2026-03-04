use std::io::Write;
use std::process::Command;

fn compress_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_compress"))
}

// ─── Basic functionality ─────────────────────────────────────────────

#[test]
fn inline_input_produces_output() {
    let output = compress_bin()
        .args(["-i", "the quick brown fox jumps over the lazy dog", "-a", "0.5"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.trim().is_empty());
}

#[test]
fn stdin_input_produces_output() {
    let mut child = compress_bin()
        .args(["-a", "0.5"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"the quick brown fox jumps over the lazy dog")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.trim().is_empty());
}

#[test]
fn file_input_produces_output() {
    let tmp = std::env::temp_dir().join("compress_test_input.txt");
    std::fs::write(&tmp, "the quick brown fox jumps over the lazy dog").unwrap();

    let output = compress_bin()
        .args(["-f", tmp.to_str().unwrap(), "-a", "0.5"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.trim().is_empty());

    std::fs::remove_file(tmp).ok();
}

// ─── Output formats ──────────────────────────────────────────────────

#[test]
fn text_format_is_plain_text() {
    let output = compress_bin()
        .args(["-i", "hello world test input", "-a", "0.3", "--format", "text"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should NOT be JSON
    assert!(!stdout.trim().starts_with('{'));
}

#[test]
fn json_format_returns_valid_json() {
    let output = compress_bin()
        .args(["-i", "the quick brown fox jumps over the lazy dog", "-a", "0.5", "--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(json["output"].is_string());
    assert!(json["output_tokens"].is_number());
    assert!(json["original_input_tokens"].is_number());
    assert!(json["compression_ratio"].is_number());
}

#[test]
fn json_format_has_valid_ratio() {
    let output = compress_bin()
        .args([
            "-i",
            "the quick brown fox jumps over the lazy dog",
            "-a", "0.5",
            "--format", "json",
        ])
        .output()
        .unwrap();

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();

    let ratio = json["compression_ratio"].as_f64().unwrap();
    assert!(ratio >= 0.0 && ratio <= 1.0, "ratio {ratio} out of bounds");
}

// ─── Stats flag ──────────────────────────────────────────────────────

#[test]
fn stats_flag_prints_to_stderr() {
    let output = compress_bin()
        .args(["-i", "the quick brown fox", "-a", "0.5", "--stats"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Original tokens:"), "stderr: {stderr}");
    assert!(stderr.contains("Compressed tokens:"), "stderr: {stderr}");
    assert!(stderr.contains("Compression ratio:"), "stderr: {stderr}");
    assert!(stderr.contains("Tokens saved:"), "stderr: {stderr}");
}

#[test]
fn no_stats_flag_produces_clean_stdout() {
    let output = compress_bin()
        .args(["-i", "the quick brown fox", "-a", "0.5"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    // Without --stats, stderr should be empty (or just tracing noise)
    assert!(!stderr.contains("Original tokens:"));
}

// ─── Aggressiveness levels ───────────────────────────────────────────

#[test]
fn aggressiveness_zero_returns_input() {
    let input = "hello world";
    let output = compress_bin()
        .args(["-i", input, "-a", "0.0"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.trim(), input);
}

#[test]
fn higher_aggressiveness_produces_shorter_output() {
    let input = "the quick brown fox jumps over the lazy dog and it was a very good day for everyone in the park";

    let low = compress_bin()
        .args(["-i", input, "-a", "0.3", "--format", "json"])
        .output()
        .unwrap();
    let high = compress_bin()
        .args(["-i", input, "-a", "0.8", "--format", "json"])
        .output()
        .unwrap();

    let low_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(low.stdout).unwrap()).unwrap();
    let high_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(high.stdout).unwrap()).unwrap();

    let low_tokens = low_json["output_tokens"].as_u64().unwrap();
    let high_tokens = high_json["output_tokens"].as_u64().unwrap();

    assert!(
        low_tokens >= high_tokens,
        "low aggressiveness ({low_tokens}) should produce >= tokens than high ({high_tokens})"
    );
}

// ─── Safe tags via CLI ───────────────────────────────────────────────

#[test]
fn safe_tags_survive_compression() {
    let output = compress_bin()
        .args([
            "-i",
            "remove the filler but <ttc_safe>keep this</ttc_safe> always",
            "-a", "0.9",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("keep"), "safe word 'keep' must survive");
    assert!(stdout.contains("this"), "safe word 'this' must survive");
}

// ─── Error cases ─────────────────────────────────────────────────────

#[test]
fn missing_file_exits_with_error() {
    let output = compress_bin()
        .args(["-f", "/nonexistent/file.txt"])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn no_input_exits_with_error() {
    // Provide empty stdin
    let mut child = compress_bin()
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    // Close stdin immediately
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("no input") || stderr.contains("empty"), "stderr: {stderr}");
}

// ─── Version and help ────────────────────────────────────────────────

#[test]
fn help_flag_exits_successfully() {
    let output = compress_bin().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("compress"));
    assert!(stdout.contains("aggressiveness"));
}

#[test]
fn version_flag_exits_successfully() {
    let output = compress_bin().arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("0.1.0"));
}

// ─── Target model flag ──────────────────────────────────────────────

#[test]
fn custom_target_model_flag() {
    let output = compress_bin()
        .args(["-i", "hello world", "-m", "claude", "--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(json["output_tokens"].is_number());
}
