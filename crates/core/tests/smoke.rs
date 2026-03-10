mod support;

use std::fs;

use support::{assert_err_contains, do_edit, fixture_path, parse_json, run_linehash, tmpfile};
#[cfg(unix)]
use support::{chmod, mode};

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
    let (stdout, stderr, code) =
        run_linehash(&["read", &fixture_arg, "--anchor", &anchor, "--context", "1"]);

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
    assert_err_contains(
        &["read", "/definitely/missing/file.txt", "--anchor", "bogus"],
        "I/O error:",
    );
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
    assert!(
        parsed
            .as_array()
            .unwrap()
            .iter()
            .all(|line| line["content"] != "function greet(name) {")
    );
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
    let (stdout, stderr, code) =
        run_linehash(&["annotate", &fixture_arg, "definitely-not-present"]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(stdout, "No matches found.\n");
}

#[test]
fn annotate_invalid_regex_reports_error() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    assert_err_contains(
        &["annotate", &fixture_arg, "(", "--regex"],
        "invalid pattern '('",
    );
}

#[test]
fn edit_single_line_updates_file_contents() {
    let edited = do_edit(
        "alpha\nbeta\n",
        &anchor_for_line("alpha\nbeta\n", 2),
        "gamma",
    );
    assert_eq!(edited, "alpha\ngamma\n");
}

#[test]
fn edit_range_replaces_lines_with_single_line() {
    let content = "alpha\nbeta\ngamma\ndelta\n";
    let start = anchor_for_line(content, 2);
    let end = anchor_for_line(content, 3);
    let edited = do_edit(content, &format!("{start}..{end}"), "merged");
    assert_eq!(edited, "alpha\nmerged\ndelta\n");
}

#[test]
fn edit_dry_run_reports_change_without_writing_file() {
    let file = tmpfile("alpha\nbeta\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 2);
    let (stdout, stderr, code) = run_linehash(&["edit", &file_arg, &anchor, "gamma", "--dry-run"]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("Would change line 2:"));
    assert!(stdout.contains("No file was written."));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\n");
}

#[test]
fn edit_json_dry_run_returns_proposed_document() {
    let file = tmpfile("alpha\nbeta\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 2);
    let parsed = parse_json(&["edit", &file_arg, &anchor, "gamma", "--dry-run", "--json"]);

    assert_eq!(parsed["lines"][1]["content"], "gamma");
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\n");
}

#[test]
fn edit_expect_mtime_rejects_stale_file() {
    let file = tmpfile("alpha\nbeta\n");
    let file_arg = file.to_string_lossy().into_owned();
    let parsed = parse_json(&["read", &file_arg, "--json"]);
    let stale_mtime = parsed["mtime"].as_i64().unwrap() - 1;
    let anchor = anchor_from_file(&file_arg, 2);
    let (_stdout, stderr, code) = run_linehash(&[
        "edit",
        &file_arg,
        &anchor,
        "gamma",
        "--expect-mtime",
        &stale_mtime.to_string(),
    ]);

    assert_eq!(code, 1);
    assert!(stderr.contains("changed since the last read"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\n");
}

#[test]
fn edit_expect_inode_rejects_stale_file() {
    let file = tmpfile("alpha\nbeta\n");
    let file_arg = file.to_string_lossy().into_owned();
    let parsed = parse_json(&["read", &file_arg, "--json"]);
    let stale_inode = parsed["inode"].as_u64().unwrap() + 1;
    let anchor = anchor_from_file(&file_arg, 2);
    let (_stdout, stderr, code) = run_linehash(&[
        "edit",
        &file_arg,
        &anchor,
        "gamma",
        "--expect-inode",
        &stale_inode.to_string(),
    ]);

    assert_eq!(code, 1);
    assert!(stderr.contains("changed since the last read"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\n");
}

#[test]
fn edit_accepts_matching_mtime_and_inode_guards() {
    let file = tmpfile("alpha\nbeta\n");
    let file_arg = file.to_string_lossy().into_owned();
    let parsed = parse_json(&["read", &file_arg, "--json"]);
    let anchor = anchor_from_file(&file_arg, 2);
    let (stdout, stderr, code) = run_linehash(&[
        "edit",
        &file_arg,
        &anchor,
        "gamma",
        "--expect-mtime",
        &parsed["mtime"].as_i64().unwrap().to_string(),
        "--expect-inode",
        &parsed["inode"].as_u64().unwrap().to_string(),
    ]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("Edited line 2."));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\ngamma\n");
}

#[test]
fn insert_after_anchor_updates_file_contents() {
    let file = tmpfile("alpha\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 1);
    let (stdout, stderr, code) = run_linehash(&["insert", &file_arg, &anchor, "beta"]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(stdout, "Inserted line 2.\n");
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\ngamma\n");
}

#[test]
fn insert_before_anchor_updates_file_contents() {
    let file = tmpfile("alpha\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 2);
    let (stdout, stderr, code) = run_linehash(&["insert", &file_arg, &anchor, "beta", "--before"]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(stdout, "Inserted line 2.\n");
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\ngamma\n");
}

#[test]
fn insert_dry_run_reports_change_without_writing_file() {
    let file = tmpfile("alpha\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 1);
    let (stdout, stderr, code) = run_linehash(&["insert", &file_arg, &anchor, "beta", "--dry-run"]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("Would insert line 2 after line 1:"));
    assert!(stdout.contains("No file was written."));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\ngamma\n");
}

#[test]
fn insert_json_dry_run_returns_proposed_document() {
    let file = tmpfile("alpha\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 1);
    let parsed = parse_json(&["insert", &file_arg, &anchor, "beta", "--dry-run", "--json"]);

    assert_eq!(parsed["lines"][1]["content"], "beta");
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\ngamma\n");
}

#[test]
fn insert_expect_mtime_rejects_stale_file() {
    let file = tmpfile("alpha\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let parsed = parse_json(&["read", &file_arg, "--json"]);
    let stale_mtime = parsed["mtime"].as_i64().unwrap() - 1;
    let anchor = anchor_from_file(&file_arg, 1);
    let (_stdout, stderr, code) = run_linehash(&[
        "insert",
        &file_arg,
        &anchor,
        "beta",
        "--expect-mtime",
        &stale_mtime.to_string(),
    ]);

    assert_eq!(code, 1);
    assert!(stderr.contains("changed since the last read"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\ngamma\n");
}

#[test]
fn insert_expect_inode_rejects_stale_file() {
    let file = tmpfile("alpha\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let parsed = parse_json(&["read", &file_arg, "--json"]);
    let stale_inode = parsed["inode"].as_u64().unwrap() + 1;
    let anchor = anchor_from_file(&file_arg, 1);
    let (_stdout, stderr, code) = run_linehash(&[
        "insert",
        &file_arg,
        &anchor,
        "beta",
        "--expect-inode",
        &stale_inode.to_string(),
    ]);

    assert_eq!(code, 1);
    assert!(stderr.contains("changed since the last read"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\ngamma\n");
}

#[test]
fn insert_accepts_matching_mtime_and_inode_guards() {
    let file = tmpfile("alpha\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let parsed = parse_json(&["read", &file_arg, "--json"]);
    let anchor = anchor_from_file(&file_arg, 1);
    let (stdout, stderr, code) = run_linehash(&[
        "insert",
        &file_arg,
        &anchor,
        "beta",
        "--expect-mtime",
        &parsed["mtime"].as_i64().unwrap().to_string(),
        "--expect-inode",
        &parsed["inode"].as_u64().unwrap().to_string(),
    ]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("Inserted line 2."));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\ngamma\n");
}

#[test]
fn insert_preserves_crlf_and_trailing_newline() {
    let file = tmpfile("alpha\r\ngamma\r\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 1);
    let (_stdout, stderr, code) = run_linehash(&["insert", &file_arg, &anchor, "beta"]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "alpha\r\nbeta\r\ngamma\r\n"
    );
}

#[test]
fn delete_removes_resolved_line() {
    let file = tmpfile("alpha\nbeta\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 2);
    let (stdout, stderr, code) = run_linehash(&["delete", &file_arg, &anchor]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(stdout, "Deleted line 2.\n");
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\ngamma\n");
}

#[test]
fn delete_dry_run_reports_change_without_writing_file() {
    let file = tmpfile("alpha\nbeta\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 2);
    let (stdout, stderr, code) = run_linehash(&["delete", &file_arg, &anchor, "--dry-run"]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("Would delete line 2:"));
    assert!(stdout.contains("No file was written."));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\ngamma\n");
}

#[test]
fn delete_json_dry_run_returns_proposed_document() {
    let file = tmpfile("alpha\nbeta\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 2);
    let parsed = parse_json(&["delete", &file_arg, &anchor, "--dry-run", "--json"]);

    assert_eq!(parsed["lines"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["lines"][1]["content"], "gamma");
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\ngamma\n");
}

#[test]
fn delete_expect_mtime_rejects_stale_file() {
    let file = tmpfile("alpha\nbeta\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let parsed = parse_json(&["read", &file_arg, "--json"]);
    let stale_mtime = parsed["mtime"].as_i64().unwrap() - 1;
    let anchor = anchor_from_file(&file_arg, 2);
    let (_stdout, stderr, code) = run_linehash(&[
        "delete",
        &file_arg,
        &anchor,
        "--expect-mtime",
        &stale_mtime.to_string(),
    ]);

    assert_eq!(code, 1);
    assert!(stderr.contains("changed since the last read"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\ngamma\n");
}

#[test]
fn delete_expect_inode_rejects_stale_file() {
    let file = tmpfile("alpha\nbeta\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let parsed = parse_json(&["read", &file_arg, "--json"]);
    let stale_inode = parsed["inode"].as_u64().unwrap() + 1;
    let anchor = anchor_from_file(&file_arg, 2);
    let (_stdout, stderr, code) = run_linehash(&[
        "delete",
        &file_arg,
        &anchor,
        "--expect-inode",
        &stale_inode.to_string(),
    ]);

    assert_eq!(code, 1);
    assert!(stderr.contains("changed since the last read"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\ngamma\n");
}

#[test]
fn delete_accepts_matching_mtime_and_inode_guards() {
    let file = tmpfile("alpha\nbeta\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let parsed = parse_json(&["read", &file_arg, "--json"]);
    let anchor = anchor_from_file(&file_arg, 2);
    let (stdout, stderr, code) = run_linehash(&[
        "delete",
        &file_arg,
        &anchor,
        "--expect-mtime",
        &parsed["mtime"].as_i64().unwrap().to_string(),
        "--expect-inode",
        &parsed["inode"].as_u64().unwrap().to_string(),
    ]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("Deleted line 2."));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\ngamma\n");
}

#[test]
fn delete_last_remaining_line_produces_empty_file() {
    let file = tmpfile("alpha");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 1);
    let (_stdout, stderr, code) = run_linehash(&["delete", &file_arg, &anchor]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(fs::read_to_string(&file).unwrap(), "");
}

#[cfg(unix)]
#[test]
fn edit_preserves_existing_file_permissions() {
    let file = tmpfile("alpha\nbeta\n");
    chmod(&file, 0o640);
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 2);

    let (_stdout, stderr, code) = run_linehash(&["edit", &file_arg, &anchor, "gamma"]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\ngamma\n");
    assert_eq!(mode(&file), 0o640);
}

#[cfg(unix)]
#[test]
fn delete_to_empty_file_preserves_existing_permissions() {
    let file = tmpfile("alpha");
    chmod(&file, 0o600);
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 1);

    let (_stdout, stderr, code) = run_linehash(&["delete", &file_arg, &anchor]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(fs::read_to_string(&file).unwrap(), "");
    assert_eq!(mode(&file), 0o600);
}

#[test]
fn patch_applies_edit_insert_and_delete_atomically() {
    let file = tmpfile("alpha\nbeta\ngamma\ndelta\n");
    let file_arg = file.to_string_lossy().into_owned();
    let edit_anchor = anchor_from_file(&file_arg, 2);
    let insert_anchor = anchor_from_file(&file_arg, 2);
    let delete_anchor = anchor_from_file(&file_arg, 4);
    let patch_file = tmpfile(&format!(
        "{{\n  \"file\": {:?},\n  \"ops\": [\n    {{ \"op\": \"edit\", \"anchor\": {:?}, \"content\": \"BETA\" }},\n    {{ \"op\": \"insert\", \"anchor\": {:?}, \"content\": \"between\" }},\n    {{ \"op\": \"delete\", \"anchor\": {:?} }}\n  ]\n}}\n",
        file_arg, edit_anchor, insert_anchor, delete_anchor
    ));
    let patch_arg = patch_file.to_string_lossy().into_owned();
    let (stdout, stderr, code) = run_linehash(&["patch", &file_arg, &patch_arg]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("Applied 3 ops: 1 edit, 1 insert, 1 delete."));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "alpha\nBETA\nbetween\ngamma\n"
    );
}

#[test]
fn patch_dry_run_does_not_modify_file() {
    let file = tmpfile("alpha\nbeta\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let edit_anchor = anchor_from_file(&file_arg, 2);
    let patch_file = tmpfile(&format!(
        "{{\"ops\":[{{\"op\":\"edit\",\"anchor\":{:?},\"content\":\"BETA\"}}]}}",
        edit_anchor
    ));
    let patch_arg = patch_file.to_string_lossy().into_owned();
    let (stdout, stderr, code) = run_linehash(&["patch", &file_arg, &patch_arg, "--dry-run"]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("Would apply applied 1 ops: 1 edit, 0 inserts, 0 deletes."));
    assert!(stdout.contains("No file was written."));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\ngamma\n");
}

#[test]
fn patch_json_dry_run_returns_proposed_document() {
    let file = tmpfile("alpha\nbeta\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let edit_anchor = anchor_from_file(&file_arg, 2);
    let patch_file = tmpfile(&format!(
        "{{\"ops\":[{{\"op\":\"edit\",\"anchor\":{:?},\"content\":\"BETA\"}}]}}",
        edit_anchor
    ));
    let patch_arg = patch_file.to_string_lossy().into_owned();
    let parsed = parse_json(&["patch", &file_arg, &patch_arg, "--dry-run", "--json"]);

    assert_eq!(parsed["lines"][1]["content"], "BETA");
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\ngamma\n");
}

#[test]
fn patch_respects_matching_guards() {
    let file = tmpfile("alpha\nbeta\n");
    let file_arg = file.to_string_lossy().into_owned();
    let parsed = parse_json(&["read", &file_arg, "--json"]);
    let edit_anchor = anchor_from_file(&file_arg, 2);
    let patch_file = tmpfile(&format!(
        "{{\"ops\":[{{\"op\":\"edit\",\"anchor\":{:?},\"content\":\"gamma\"}}]}}",
        edit_anchor
    ));
    let patch_arg = patch_file.to_string_lossy().into_owned();
    let (stdout, stderr, code) = run_linehash(&[
        "patch",
        &file_arg,
        &patch_arg,
        "--expect-mtime",
        &parsed["mtime"].as_i64().unwrap().to_string(),
        "--expect-inode",
        &parsed["inode"].as_u64().unwrap().to_string(),
    ]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("Applied 1 ops: 1 edit, 0 inserts, 0 deletes."));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\ngamma\n");
}

#[test]
fn patch_rejects_stale_guard_without_writing() {
    let file = tmpfile("alpha\nbeta\n");
    let file_arg = file.to_string_lossy().into_owned();
    let parsed = parse_json(&["read", &file_arg, "--json"]);
    let edit_anchor = anchor_from_file(&file_arg, 2);
    let patch_file = tmpfile(&format!(
        "{{\"ops\":[{{\"op\":\"edit\",\"anchor\":{:?},\"content\":\"gamma\"}}]}}",
        edit_anchor
    ));
    let patch_arg = patch_file.to_string_lossy().into_owned();
    let (_stdout, stderr, code) = run_linehash(&[
        "patch",
        &file_arg,
        &patch_arg,
        "--expect-mtime",
        &(parsed["mtime"].as_i64().unwrap() - 1).to_string(),
    ]);

    assert_eq!(code, 1);
    assert!(stderr.contains("changed since the last read"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\n");
}

#[test]
fn patch_rejects_bad_anchor_without_writing() {
    let file = tmpfile("alpha\nbeta\n");
    let file_arg = file.to_string_lossy().into_owned();
    let patch_file = tmpfile("{\"ops\":[{\"op\":\"delete\",\"anchor\":\"9:ff\"}]}");
    let patch_arg = patch_file.to_string_lossy().into_owned();
    let (_stdout, stderr, code) = run_linehash(&["patch", &file_arg, &patch_arg]);

    assert_eq!(code, 1);
    assert!(stderr.contains("patch failed at operation 1"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\n");
}

#[test]
fn patch_reports_failing_operation_index() {
    let file = tmpfile("alpha\nbeta\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 1);
    let patch_file = tmpfile(&format!(
        "{{\"ops\":[{{\"op\":\"insert\",\"anchor\":{:?},\"content\":\"ok\"}},{{\"op\":\"delete\",\"anchor\":\"9:ff\"}}]}}",
        anchor
    ));
    let patch_arg = patch_file.to_string_lossy().into_owned();
    let (_stdout, stderr, code) = run_linehash(&["patch", &file_arg, &patch_arg]);

    assert_eq!(code, 1);
    assert!(stderr.contains("patch failed at operation 2"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\ngamma\n");
}

#[test]
fn patch_rejects_overlapping_operations_without_writing() {
    let file = tmpfile("alpha\nbeta\ngamma\ndelta\n");
    let file_arg = file.to_string_lossy().into_owned();
    let range_start = anchor_from_file(&file_arg, 2);
    let range_end = anchor_from_file(&file_arg, 3);
    let delete_anchor = anchor_from_file(&file_arg, 2);
    let patch_file = tmpfile(&format!(
        "{{\"ops\":[{{\"op\":\"edit\",\"anchor\":{:?},\"content\":\"merged\"}},{{\"op\":\"delete\",\"anchor\":{:?}}}]}}",
        format!("{range_start}..{range_end}"),
        delete_anchor
    ));
    let patch_arg = patch_file.to_string_lossy().into_owned();
    let (_stdout, stderr, code) = run_linehash(&["patch", &file_arg, &patch_arg]);

    assert_eq!(code, 1);
    assert!(stderr.contains("overlaps an earlier edit"));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "alpha\nbeta\ngamma\ndelta\n"
    );
}

#[test]
fn patch_rejects_mismatched_embedded_file_without_writing() {
    let file = tmpfile("alpha\nbeta\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 2);
    let patch_file = tmpfile(&format!(
        "{{\"file\":\"/definitely/other.txt\",\"ops\":[{{\"op\":\"edit\",\"anchor\":{:?},\"content\":\"gamma\"}}]}}",
        anchor
    ));
    let patch_arg = patch_file.to_string_lossy().into_owned();
    let (_stdout, stderr, code) = run_linehash(&["patch", &file_arg, &patch_arg]);

    assert_eq!(code, 1);
    assert!(stderr.contains("operation 0"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\n");
}

#[test]
fn patch_uses_original_snapshot_for_later_ops() {
    let file = tmpfile("alpha\nbeta\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let first_anchor = anchor_from_file(&file_arg, 1);
    let second_anchor = anchor_from_file(&file_arg, 2);
    let patch_file = tmpfile(&format!(
        "{{\"ops\":[{{\"op\":\"insert\",\"anchor\":{:?},\"content\":\"before-beta\"}},{{\"op\":\"edit\",\"anchor\":{:?},\"content\":\"BETA\"}}]}}",
        first_anchor, second_anchor
    ));
    let patch_arg = patch_file.to_string_lossy().into_owned();
    let (_stdout, stderr, code) = run_linehash(&["patch", &file_arg, &patch_arg]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "alpha\nbefore-beta\nBETA\ngamma\n"
    );
}

#[test]
fn patch_multiple_inserts_at_same_anchor_preserve_order() {
    let file = tmpfile("alpha\nbeta\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 1);
    let patch_file = tmpfile(&format!(
        "{{\"ops\":[{{\"op\":\"insert\",\"anchor\":{:?},\"content\":\"first\"}},{{\"op\":\"insert\",\"anchor\":{:?},\"content\":\"second\"}}]}}",
        anchor, anchor
    ));
    let patch_arg = patch_file.to_string_lossy().into_owned();
    let (_stdout, stderr, code) = run_linehash(&["patch", &file_arg, &patch_arg]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "alpha\nfirst\nsecond\nbeta\n"
    );
}

#[test]
fn patch_preserves_crlf_and_trailing_newline() {
    let file = tmpfile("alpha\r\nbeta\r\n");
    let file_arg = file.to_string_lossy().into_owned();
    let anchor = anchor_from_file(&file_arg, 2);
    let patch_file = tmpfile(&format!(
        "{{\"ops\":[{{\"op\":\"edit\",\"anchor\":{:?},\"content\":\"gamma\"}}]}}",
        anchor
    ));
    let patch_arg = patch_file.to_string_lossy().into_owned();
    let (_stdout, stderr, code) = run_linehash(&["patch", &file_arg, &patch_arg]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\r\ngamma\r\n");
}

#[test]
fn stats_pretty_output_reports_summary_fields() {
    let file = tmpfile("alpha\nbeta\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let (stdout, stderr, code) = run_linehash(&["stats", &file_arg]);

    assert_eq!(code, 0, "expected success, got stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("Lines: 3"));
    assert!(stdout.contains("Unique hashes (2-char):"));
    assert!(stdout.contains("Collisions:"));
    assert!(stdout.contains("Est. read tokens:"));
    assert!(stdout.contains("Hash length advice:"));
    assert!(stdout.contains("Suggested --context:"));
}

#[test]
fn stats_json_output_is_structured() {
    let file = tmpfile("alpha\nbeta\ngamma\n");
    let file_arg = file.to_string_lossy().into_owned();
    let parsed = parse_json(&["stats", &file_arg, "--json"]);

    assert_eq!(parsed["line_count"], 3);
    assert!(parsed["unique_hashes"].is_u64());
    assert!(parsed["collision_count"].is_u64());
    assert!(parsed["collision_pairs"].is_array());
    assert!(parsed["estimated_read_tokens"].is_u64());
    assert!(parsed["hash_length_advice"].is_u64());
    assert!(parsed["suggested_context_n"].is_u64());
}

#[test]
fn stats_reports_collision_pairs_for_collision_file() {
    let (first, second) = find_collision_pair();
    let file = tmpfile(&format!("{first}\n{second}\nunique\n"));
    let file_arg = file.to_string_lossy().into_owned();
    let parsed = parse_json(&["stats", &file_arg, "--json"]);

    assert_eq!(parsed["collision_count"], 2);
    assert_eq!(parsed["collision_pairs"][0][0], 1);
    assert_eq!(parsed["collision_pairs"][0][1], 2);
}

#[test]
fn helper_tmpfile_writes_expected_content() {
    let file = tmpfile("alpha\nbeta\n");
    let contents = std::fs::read_to_string(&file).unwrap();
    assert_eq!(contents, "alpha\nbeta\n");
}

fn anchor_for_line(content: &str, line_no: usize) -> String {
    let file = tmpfile(content);
    let file_arg = file.to_string_lossy().into_owned();
    anchor_from_file(&file_arg, line_no)
}

fn anchor_from_file(file_arg: &str, line_no: usize) -> String {
    let parsed = parse_json(&["read", file_arg, "--json"]);
    format!(
        "{}:{}",
        line_no,
        parsed["lines"][line_no - 1]["hash"].as_str().unwrap()
    )
}

fn find_collision_pair() -> (String, String) {
    use std::collections::HashMap;
    use xxhash_rust::xxh32::xxh32;

    let mut seen: HashMap<String, String> = HashMap::new();
    for i in 0..10_000 {
        let candidate = format!("line-{i}");
        let hash = format!("{:02x}", xxh32(candidate.as_bytes(), 0) & 0xff);
        if let Some(existing) = seen.insert(hash, candidate.clone()) {
            if existing != candidate {
                return (existing, candidate);
            }
        }
    }

    panic!("failed to find a short-hash collision in search space");
}
