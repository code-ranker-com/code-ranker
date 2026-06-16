//! Tests for `python/structure.rs` (wired via `#[path]` from that source).

use super::*;

#[test]
fn py_is_test_path_matches_conventions() {
    for p in [
        "tests/test_x.py",
        "pkg/tests/helper.py",
        "test/helper.py",
        "conftest.py",
        "pkg/conftest.py",
        "test_module.py",
        "pkg/test_module.py",
        "module_test.py",
    ] {
        assert!(py_is_test_path(p), "should be a test: {p}");
    }
    for p in [
        "pkg/module.py",
        "pkg/contest.py",    // not a `tests` dir component
        "pkg/test_data.txt", // `test_` but not `.py`
        "latest.py",
    ] {
        assert!(!py_is_test_path(p), "should not be a test: {p}");
    }
}

#[test]
fn test_convention_lists_load_from_config() {
    // The moved DATA lists resolve from `python/config.toml` verbatim.
    assert_eq!(KINDS.test_dirs, ["tests", "test"]);
    assert_eq!(KINDS.test_files, ["conftest.py"]);
    assert_eq!(KINDS.test_prefixes, ["test_"]);
    assert_eq!(KINDS.test_suffixes, ["_test.py"]);
}

#[test]
fn uses_edge_kind_resolves_against_published_vocab() {
    // The tagged kind is validated against the merged `[edge_kinds]` (inherited
    // `uses` from defaults.toml) — never a bare literal.
    assert_eq!(uses_edge_kind(), "uses");
}
