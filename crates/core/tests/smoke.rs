mod support;

use support::{assert_err_contains, fixture_path, run_linehash, tmpfile};

#[test]
fn missing_file_still_hits_read_stub_for_now() {
    assert_err_contains(
        &["read", "/definitely/missing/file.txt"],
        "read is not implemented yet",
    );
}

#[test]
fn read_fixture_returns_not_implemented_error_for_now() {
    let fixture = fixture_path("simple_lf.js");
    let fixture_arg = fixture.to_string_lossy().into_owned();
    let (_stdout, stderr, code) = run_linehash(&["read", &fixture_arg]);

    assert_eq!(code, 1);
    assert!(stderr.contains("read is not implemented yet"));
}

#[test]
fn helper_tmpfile_writes_expected_content() {
    let file = tmpfile("alpha\nbeta\n");
    let contents = std::fs::read_to_string(&file).unwrap();
    assert_eq!(contents, "alpha\nbeta\n");
}
