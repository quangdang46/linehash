mod support;

use support::{assert_err_contains, fixture_path, parse_json, run_linehash, tmpfile};

#[test]
fn missing_file_read_reports_io_error() {
    let (_stdout, stderr, code) = run_linehash(&["read", "/definitely/missing/file.txt"]);

    assert_eq!(code, 1);
    assert!(stderr.contains("Error: I/O error:"));
}

#[test]
fn read_fixture_pretty_output_includes_anchors() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let (stdout, stderr, code) = run_linehash(&["read", &fixture_arg]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("1:"));
    assert!(stdout.contains("| function greet(name) {"));
    assert!(stdout.contains("| export function main() {"));
}

#[test]
fn read_json_includes_file_metadata_and_lines() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let parsed = parse_json(&["read", &fixture_arg, "--json"]);

    assert_eq!(parsed["file"], fixture_arg);
    assert_eq!(parsed["newline"], "lf");
    assert_eq!(parsed["trailing_newline"], true);
    assert!(parsed["mtime"].is_i64());
    assert!(parsed["mtime_nanos"].is_u64());
    assert!(parsed["inode"].is_u64());
    assert_eq!(parsed["lines"][0]["n"], 1);
    assert_eq!(parsed["lines"][0]["content"], "function greet(name) {");
}

#[test]
fn read_anchor_context_only_shows_neighborhood() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let full = parse_json(&["read", &fixture_arg, "--json"]);
    let anchor = format!("7:{}", full["lines"][6]["hash"].as_str().unwrap());
    let (stdout, stderr, code) = run_linehash(&["read", &fixture_arg, "--anchor", &anchor, "--context", "1"]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("→7:"));
    assert!(stdout.contains(" 6:"));
    assert!(stdout.contains(" 8:"));
    assert!(!stdout.contains(" 1:"));
    assert!(!stdout.contains(" 9:"));
}

#[test]
fn index_pretty_output_shows_hashes_only() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let (stdout, stderr, code) = run_linehash(&["index", &fixture_arg]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.lines().all(|line| !line.contains("|")));
    assert!(stdout.lines().all(|line| line.split(':').count() == 2));
}

#[test]
fn index_json_output_is_stable() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let parsed = parse_json(&["index", &fixture_arg, "--json"]);

    assert_eq!(parsed["file"], fixture_arg);
    assert_eq!(parsed["lines"][0]["n"], 1);
    assert!(parsed["lines"][0]["hash"].is_string());
    assert!(parsed["lines"][0].get("content").is_none());
}

#[test]
fn invalid_anchor_still_errors_for_read_context() {
    assert_err_contains(&["read", "/definitely/missing/file.txt", "--anchor", "bogus"], "I/O error:");
}

#[test]
fn verify_all_valid_anchors_exits_zero() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let full = parse_json(&["read", &fixture_arg, "--json"]);
    let anchor_a = format!("1:{}", full["lines"][0]["hash"].as_str().unwrap());
    let anchor_b = format!("7:{}", full["lines"][6]["hash"].as_str().unwrap());
    let (stdout, stderr, code) = run_linehash(&["verify", &fixture_arg, &anchor_a, &anchor_b]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("✓  1:"));
    assert!(stdout.contains("✓  7:"));
}

#[test]
fn verify_mixed_results_exit_nonzero() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let full = parse_json(&["read", &fixture_arg, "--json"]);
    let valid = format!("1:{}", full["lines"][0]["hash"].as_str().unwrap());
    let stale = "7:ff";
    let (stdout, stderr, code) = run_linehash(&["verify", &fixture_arg, &valid, stale]);

    assert_eq!(code, 1);
    assert!(stderr.is_empty());
    assert!(stdout.contains("✓  1:"));
    assert!(stdout.contains("✗  7:ff"));
    assert!(stdout.contains("expected hash ff"));
}

#[test]
fn verify_json_output_is_structured() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let full = parse_json(&["read", &fixture_arg, "--json"]);
    let valid = format!("1:{}", full["lines"][0]["hash"].as_str().unwrap());
    let (stdout, stderr, code) = run_linehash(&["verify", &fixture_arg, &valid, "bogus", "--json"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(code, 1);
    assert!(stderr.is_empty());
    assert!(parsed.is_array());
    assert_eq!(parsed[0]["status"], "ok");
    assert_eq!(parsed[0]["line_no"], 1);
    assert_eq!(parsed[1]["status"], "parse_error");
    assert!(parsed[1]["error"].is_string());
}

#[test]
fn grep_pretty_returns_anchor_formatted_matches() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let (stdout, stderr, code) = run_linehash(&["grep", &fixture_arg, "greet"]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("1:"));
    assert!(stdout.contains("| function greet(name) {"));
    assert!(stdout.contains("|   return greet(name)"));
}

#[test]
fn grep_json_returns_filtered_lines_array() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let parsed = parse_json(&["grep", &fixture_arg, "world", "--json"]);

    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 1);
    assert_eq!(parsed[0]["n"], 7);
    assert_eq!(parsed[0]["content"], "  const name = 'world'");
}

#[test]
fn grep_invalid_regex_reports_error() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    assert_err_contains(&["grep", &fixture_arg, "("], "invalid pattern '('");
}

#[test]
fn grep_invert_returns_non_matching_lines() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let parsed = parse_json(&["grep", &fixture_arg, "greet", "--invert", "--json"]);

    assert!(parsed.is_array());
    assert!(parsed.as_array().unwrap().len() < 9);
    assert!(parsed.as_array().unwrap().iter().all(|line| line["content"] != "function greet(name) {"));
}

#[test]
fn annotate_substring_match_returns_anchor_output() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let (stdout, stderr, code) = run_linehash(&["annotate", &fixture_arg, "greet(name)"]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("1:"));
    assert!(stdout.contains("| function greet(name) {"));
    assert!(stdout.contains("|   return greet(name)"));
}

#[test]
fn annotate_regex_mode_returns_matches() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let parsed = parse_json(&["annotate", &fixture_arg, "^export", "--regex", "--json"]);

    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 1);
    assert_eq!(parsed[0]["n"], 6);
    assert_eq!(parsed[0]["content"], "export function main() {");
}

#[test]
fn annotate_expect_one_with_multiple_matches_reports_candidates() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let (stdout, stderr, code) = run_linehash(&["annotate", &fixture_arg, "greet", "--expect-one"]);

    assert_eq!(code, 1, "expected ambiguity failure, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("annotate: expected 1 match, found 2"));
    assert!(stdout.contains("1:"));
    assert!(stdout.contains("8:"));
}

#[test]
fn annotate_no_match_reports_helpful_message() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let (stdout, stderr, code) = run_linehash(&["annotate", &fixture_arg, "definitely-not-present"]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(stdout, "No matches found.\n");
}

#[test]
fn annotate_invalid_regex_reports_error() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    assert_err_contains(&["annotate", &fixture_arg, "(", "--regex"], "invalid pattern '('");
}

#[test]
fn helper_tmpfile_writes_expected_content() {
    let file = tmpfile("alpha\nbeta\n");
    let contents = std::fs::read_to_string(&file).unwrap();
    assert_eq!(contents, "alpha\nbeta\n");
}
