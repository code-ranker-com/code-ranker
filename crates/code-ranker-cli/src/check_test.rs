use super::*;

fn viol(location: &str, line: Option<u32>) -> config::Violation {
    config::Violation {
        language: "rust".into(),
        rule: "threshold.file.loc".into(),
        group: "SIZ".into(),
        graph: "files",
        location: location.into(),
        line,
        message: "source loc 1318 exceeds limit 150".into(),
        weight: 8.78,
        why: None,
        fix: None,
    }
}

/// Match a target-relative violation path against `--focus-path` entries, the way
/// the gate does. The target is an unrelated absolute root, so these bare entries
/// resolve to the printed-path (legacy) match — file-exact and folder-prefix.
fn path_matches(rel: &str, focus: &[String]) -> bool {
    crate::recommend::FocusPaths::new(focus, "/ws").matches_target_rel(rel)
}

#[test]
fn path_matches_file_exactly_and_folder_prefix() {
    let focus = vec![
        "crates/a/src/plugin.rs".to_string(),
        "crates/b/src".to_string(),
    ];
    // Exact file match.
    assert!(path_matches("crates/a/src/plugin.rs", &focus));
    // Folder matches everything beneath it.
    assert!(path_matches("crates/b/src/registry.rs", &focus));
    // Outside the focused paths.
    assert!(!path_matches("crates/c/src/lib.rs", &focus));
    // A folder must match on a path boundary, not a bare prefix.
    assert!(!path_matches("crates/b/src_extra.rs", &focus));
}

#[test]
fn path_matches_ignores_leading_dot_slash_and_trailing_slash() {
    let focus = vec!["./crates/a/".to_string()];
    assert!(path_matches("crates/a/src/x.rs", &focus));
    assert!(path_matches("crates/a", &focus));
}

/// The normalization fix at the gate: `--focus-path` written as the analysis
/// target (`[input]`) scopes to it — matching every violation under the target —
/// instead of requiring the counter-intuitive target-relative form.
#[test]
fn path_matches_accepts_the_target_path_itself() {
    let fp = crate::recommend::FocusPaths::new(&["/ws/crates/a".to_string()], "/ws/crates/a");
    // Violations under the target are relativized to `src/…`; the target path
    // now matches them all.
    assert!(fp.matches_target_rel("src/store.rs"));
    assert!(fp.matches_target_rel("src/repo/mod.rs"));
}

#[test]
fn rule_matches_full_id_bare_id_and_group() {
    let v = config::Violation {
        language: "rust".into(),
        rule: "check.inline_tests_too_large".into(),
        group: "TST".into(),
        graph: "files",
        location: "{target}/src/x.rs".into(),
        line: None,
        message: "m".into(),
        weight: 1.0,
        why: None,
        fix: None,
    };
    assert!(rule_matches(&v, &["check.inline_tests_too_large".into()])); // full id
    assert!(rule_matches(&v, &["inline_tests_too_large".into()])); // bare id
    assert!(rule_matches(&v, &["TST".into()])); // concern group
    assert!(!rule_matches(&v, &["CPL".into()]));
    assert!(!rule_matches(&v, &["threshold.file.hk".into()]));
}

#[test]
fn focus_scope_note_covers_path_rule_both_and_neither() {
    assert_eq!(focus_scope_note(&[], &[]), "");
    assert_eq!(
        focus_scope_note(&["src/a.rs".into()], &[]),
        " (focused on path src/a.rs)"
    );
    assert_eq!(
        focus_scope_note(&[], &["CPL".into()]),
        " (focused on rule CPL)"
    );
    assert_eq!(
        focus_scope_note(
            &["src/a.rs".into(), "src/b".into()],
            &["CPL".into(), "SIZ".into()]
        ),
        " (focused on path src/a.rs, src/b; rule CPL, SIZ)"
    );
}

#[test]
fn print_human_diagnostics_handles_clean_and_truncated_runs() {
    let na = BTreeMap::new();
    let ck = BTreeMap::new();
    // total == 0 → the clean "no violations" line, then return.
    print_human_diagnostics(&[], 0, "rust", "proj", "", &na, &ck);
    // Showing fewer than total → the "showing the N worst" note + "(shown)" tail.
    let shown = [viol("{target}/src/x.rs", Some(7))];
    print_human_diagnostics(
        &shown,
        3,
        "rust",
        "proj",
        " (focused on rule SIZ)",
        &na,
        &ck,
    );
    // Showing all → the "(total)" tail branch.
    print_human_diagnostics(&shown, 1, "rust", "proj", "", &na, &ck);
}

#[test]
fn annotation_location_maps_target_path_with_line() {
    assert_eq!(
        annotation_location("{target}/crates/a/src/x.rs", Some(42)),
        "file=crates/a/src/x.rs,line=42,"
    );
}

#[test]
fn annotation_location_defaults_missing_line_to_one() {
    // Whole-file metrics carry no line → annotation pins to line 1.
    assert_eq!(
        annotation_location("{target}/src/x.rs", None),
        "file=src/x.rs,line=1,"
    );
}

#[test]
fn annotation_location_empty_without_a_file_path() {
    // Locationless (cycle fallback) and non-`{target}` ids stay general.
    assert_eq!(annotation_location("", Some(5)), "");
    assert_eq!(annotation_location("ext:serde", None), "");
    assert_eq!(annotation_location("{target}/", Some(1)), "");
}

#[test]
fn sarif_attaches_physical_location_from_violation() {
    let doc = sarif_document(
        &[viol("{target}/src/x.rs", Some(7))],
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    let v: serde_json::Value = serde_json::from_str(&doc).unwrap();
    let loc = &v["runs"][0]["results"][0]["locations"][0]["physicalLocation"];
    assert_eq!(loc["artifactLocation"]["uri"], "src/x.rs");
    assert_eq!(loc["region"]["startLine"], 7);
}

#[test]
fn sarif_omits_location_when_no_path() {
    let doc = sarif_document(&[viol("", None)], &BTreeMap::new(), &BTreeMap::new());
    let v: serde_json::Value = serde_json::from_str(&doc).unwrap();
    assert!(v["runs"][0]["results"][0].get("locations").is_none());
}

#[test]
fn codequality_issue_has_fingerprint_path_and_line() {
    let doc = codequality_document(&[viol("{target}/src/x.rs", Some(7))]);
    let v: serde_json::Value = serde_json::from_str(&doc).unwrap();
    let issue = &v[0];
    assert_eq!(issue["check_name"], "threshold.file.loc");
    assert_eq!(issue["severity"], "major");
    assert_eq!(issue["location"]["path"], "src/x.rs");
    assert_eq!(issue["location"]["lines"]["begin"], 7);
    // Stable identity = language:rule:location, no line (so a shift does not reopen it).
    assert_eq!(
        issue["fingerprint"],
        "rust:threshold.file.loc:{target}/src/x.rs"
    );
}

#[test]
fn codequality_whole_file_metric_defaults_line_to_one() {
    // A whole-file metric has no line → CodeClimate needs one, default 1.
    let doc = codequality_document(&[viol("{target}/src/x.rs", None)]);
    let v: serde_json::Value = serde_json::from_str(&doc).unwrap();
    assert_eq!(v[0]["location"]["lines"]["begin"], 1);
}

#[test]
fn sarif_partial_fingerprint_is_rule_and_location() {
    let doc = sarif_document(
        &[viol("{target}/src/x.rs", Some(7))],
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    let v: serde_json::Value = serde_json::from_str(&doc).unwrap();
    let fp = &v["runs"][0]["results"][0]["partialFingerprints"];
    assert_eq!(
        fp["codeRankerRuleLocation/v1"],
        "rust:threshold.file.loc:{target}/src/x.rs"
    );
}

#[test]
fn sarif_partial_fingerprint_is_stable_across_line_shifts() {
    // The same finding at a different line keeps the same fingerprint, so a
    // code shift does not reopen it for the consumer.
    let at_7 = sarif_document(
        &[viol("{target}/src/x.rs", Some(7))],
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    let at_42 = sarif_document(
        &[viol("{target}/src/x.rs", Some(42))],
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    let fp = |doc: &str| -> String {
        let v: serde_json::Value = serde_json::from_str(doc).unwrap();
        v["runs"][0]["results"][0]["partialFingerprints"]["codeRankerRuleLocation/v1"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(fp(&at_7), fp(&at_42));
}

/// A violation whose location doesn't resolve to a repo-relative path (an
/// external node, or a bare `{target}/` with nothing after it) still gets a
/// `**Module:**` line in the prompt — falling back to the raw (non-empty)
/// location instead of silently dropping the module line.
#[test]
fn render_prompt_falls_back_to_raw_location_when_not_target_relative() {
    let md = render_prompt(
        &[viol("ext:serde", None)],
        1,
        "demo",
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    assert!(
        md.contains("**Module:** `ext:serde`"),
        "falls back to the raw location: {md}"
    );
}

/// A genuinely empty location (no path at all — e.g. a project-wide finding)
/// gets no `**Module:**` line, rather than printing an empty backtick pair.
#[test]
fn render_prompt_omits_module_line_for_an_empty_location() {
    let md = render_prompt(
        &[viol("", None)],
        1,
        "demo",
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    assert!(
        !md.contains("**Module:**"),
        "no module line for an empty location: {md}"
    );
}
