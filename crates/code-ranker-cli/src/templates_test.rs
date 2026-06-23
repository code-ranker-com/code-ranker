use super::*;
use code_ranker_graph::level_graph::LevelGraph;
use code_ranker_plugin_api::Principle;
use code_ranker_plugin_api::attrs::ValueType;
use code_ranker_plugin_api::level::AttributeSpec;
use std::collections::BTreeMap;

/// A snapshot carrying just the bits `resolve_doc`/`doc_rel_path` read:
/// the principles and the `files` level's node-attribute specs.
fn snap(principles: Vec<Principle>, files_attrs: BTreeMap<String, AttributeSpec>) -> Snapshot {
    let files = LevelGraph {
        node_attributes: files_attrs,
        ..Default::default()
    };
    let mut graphs = BTreeMap::new();
    graphs.insert("files".to_string(), files);
    Snapshot::new(
        "report".into(),
        ".".into(),
        ".".into(),
        "rust".into(),
        None,
        BTreeMap::new(),
        BTreeMap::new(),
        None,
        vec![],
        graphs,
        principles,
        Default::default(),
    )
}

fn principle(id: &str, doc_url: &str) -> Principle {
    Principle {
        id: id.to_string(),
        label: id.to_string(),
        title: id.to_string(),
        prompt: String::new(),
        doc_url: Some(doc_url.to_string()),
        sort_metric: "hk".to_string(),
        connections: vec![],
    }
}

fn metric_spec(remediation: &str) -> AttributeSpec {
    let mut spec = AttributeSpec::new(ValueType::Float, "HK");
    spec.remediation = Some(remediation.to_string());
    spec
}

#[test]
fn resolve_doc_serves_base_fallback() {
    let s = snap(
        vec![principle(
            "SRP",
            "https://x/blob/main/languages/base/SRP.md",
        )],
        BTreeMap::new(),
    );
    let doc = resolve_doc(&s, &TemplatesConfig::default(), "SRP").unwrap();
    assert_eq!(doc, corpus_doc("base/SRP.md").unwrap());
}

#[test]
fn resolve_doc_assembles_a_language_manifest() {
    // rust/ADP.md is a manifest (`<!-- doc:base … -->`), so the resolved doc
    // is the composition over base/ADP.md, not the raw manifest text.
    let s = snap(
        vec![principle(
            "ADP",
            "https://x/blob/main/languages/rust/ADP.md",
        )],
        BTreeMap::new(),
    );
    let doc = resolve_doc(&s, &TemplatesConfig::default(), "ADP").unwrap();
    let manifest = corpus_doc("rust/ADP.md").unwrap();
    let base = corpus_doc("base/ADP.md").unwrap();
    let expected = crate::compose::compose(manifest, base, "Rust").unwrap();
    assert_eq!(doc, expected);
    assert!(!doc.contains("<!-- doc:base"), "includes were expanded");
}

#[test]
fn resolve_doc_manifest_uses_base_override_when_present() {
    // A `templates.languages.base.<ID>` override substitutes the neutral base that
    // a language manifest assembles over, so the custom base flows into the result.
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("base_adp.md");
    let custom_base = corpus_doc("base/ADP.md")
        .unwrap()
        .replace("Acyclic", "ZZ-MARKER-ACYCLIC");
    std::fs::write(&base_path, &custom_base).unwrap();
    let mut templates = TemplatesConfig::default();
    let mut base_overrides = BTreeMap::new();
    base_overrides.insert("ADP".to_string(), base_path.to_string_lossy().into_owned());
    templates
        .languages
        .insert("base".to_string(), base_overrides);

    let s = snap(
        vec![principle(
            "ADP",
            "https://x/blob/main/languages/rust/ADP.md",
        )],
        BTreeMap::new(),
    );
    let doc = resolve_doc(&s, &templates, "ADP").unwrap();
    let manifest = corpus_doc("rust/ADP.md").unwrap();
    let expected = crate::compose::compose(manifest, &custom_base, "Rust").unwrap();
    assert_eq!(doc, expected);
    assert!(
        doc.contains("ZZ-MARKER-ACYCLIC"),
        "custom base flowed in: {doc}"
    );
}

#[test]
fn resolve_doc_override_wins_verbatim() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("custom.md");
    std::fs::write(&path, "# my own SRP\n").unwrap();
    let mut templates = TemplatesConfig::default();
    let mut srp = BTreeMap::new();
    srp.insert("SRP".to_string(), path.to_string_lossy().into_owned());
    templates.languages.insert("rust".to_string(), srp);

    let s = snap(
        vec![principle(
            "SRP",
            "https://x/blob/main/languages/rust/SRP.md",
        )],
        BTreeMap::new(),
    );
    let doc = resolve_doc(&s, &templates, "SRP").unwrap();
    assert_eq!(doc, "# my own SRP\n");
}

#[test]
fn resolve_doc_cycle_resolves_to_adp() {
    // `cycle` is ADP's metric lens (not a node attribute), so `--doc cycle` serves
    // the ADP doc — resolved through the ADP principle, same as `--doc ADP`.
    let s = snap(
        vec![principle(
            "ADP",
            "https://x/blob/main/languages/rust/ADP.md",
        )],
        BTreeMap::new(),
    );
    let doc = resolve_doc(&s, &TemplatesConfig::default(), "cycle").unwrap();
    let manifest = corpus_doc("rust/ADP.md").unwrap();
    let base = corpus_doc("base/ADP.md").unwrap();
    let expected = crate::compose::compose(manifest, base, "Rust").unwrap();
    assert_eq!(doc, expected, "`--doc cycle` serves the ADP doc");
}

#[test]
fn resolve_doc_finds_metric_via_remediation_doc_ref() {
    // No matching principle — the doc resolves through the metric's remediation
    // `--doc <ID>` reference (the attribute looked up by its lowercased key, the
    // canonical doc filename taken from the `--doc` id). Metric docs live in base/.
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "hk".to_string(),
        metric_spec("Run `code-ranker report --doc HK` and follow its instructions."),
    );
    let s = snap(vec![], attrs);
    let doc = resolve_doc(&s, &TemplatesConfig::default(), "HK").unwrap();
    assert_eq!(doc, corpus_doc("base/HK.md").unwrap());
}

#[test]
fn resolve_doc_unknown_id_errors() {
    let s = snap(
        vec![principle(
            "SRP",
            "https://x/blob/main/languages/base/SRP.md",
        )],
        BTreeMap::new(),
    );
    let err = resolve_doc(&s, &TemplatesConfig::default(), "ZZZ").unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("no principle or metric doc"), "{msg}");
    assert!(msg.contains("SRP"), "known principles listed: {msg}");
}

#[test]
fn build_corpus_writes_every_doc_including_assembled() {
    let dir = tempfile::tempdir().unwrap();
    let n = build_corpus(dir.path()).unwrap();
    assert!(n > 0, "wrote at least one doc");

    // Base docs are copied verbatim.
    assert!(dir.path().join("base/HK.md").exists());

    // A rust manifest is published assembled, with its includes expanded.
    let assembled = std::fs::read_to_string(dir.path().join("rust/ADP.md")).unwrap();
    assert!(
        !assembled.contains("<!-- doc:base"),
        "manifest includes expanded on publish"
    );
}

#[test]
fn lang_display_maps_known_folders_and_passes_through() {
    assert_eq!(lang_display("rust"), "Rust");
    assert_eq!(lang_display("cpp"), "C++");
    assert_eq!(lang_display("csharp"), "C#");
    assert_eq!(lang_display("unknown-lang"), "unknown-lang");
}

#[test]
fn corpus_is_embedded_and_keyed_by_rel_path() {
    // The base fallback corpus is always present.
    assert!(corpus_doc("base/HK.md").is_some(), "base/HK.md embedded");
    assert!(corpus_doc("base/SRP.md").is_some(), "base/SRP.md embedded");
    assert!(corpus_doc("nope/X.md").is_none());
}

#[test]
fn url_tail_extracts_corpus_path() {
    assert_eq!(
        url_tail("https://x/blob/main/languages/base/HK.md").as_deref(),
        Some("base/HK.md")
    );
    assert_eq!(
        url_tail("Download from https://x/main/languages/rust/SRP.md now").as_deref(),
        Some("rust/SRP.md"),
        "anchored on /languages/, trailing prose trimmed"
    );
    assert_eq!(url_tail("https://x/elsewhere/HK.md"), None);
}

#[test]
fn bare_relative_path_defaults_to_base_folder() {
    // `split_once('/')` fallback: a path with no slash is treated as base/<id>.
    let (lang, file) = "HK.md".split_once('/').unwrap_or(("base", "HK.md"));
    assert_eq!((lang, file), ("base", "HK.md"));
}

#[test]
fn resolve_doc_ai_index_expands_tldr_marker() {
    // The AI overview resolves by filename fallback, and its
    // `<!-- doc:tldr-index -->` marker expands to the per-doc catalog.
    let s = snap(
        vec![principle(
            "ADP",
            "https://x/blob/main/languages/rust/ADP.md",
        )],
        BTreeMap::new(),
    );
    let doc = resolve_doc(&s, &TemplatesConfig::default(), "AI").unwrap();
    assert!(
        doc.contains("code-ranker — AI agent skill"),
        "overview head kept"
    );
    assert!(
        !doc.contains("doc:tldr-index"),
        "marker expanded, not left literal"
    );
    assert!(
        doc.contains("### ADP — Acyclic Dependencies Principle"),
        "catalog lists ADP"
    );
    assert!(
        doc.contains("Full doc: `code-ranker report --doc ADP`"),
        "each entry points at its --doc id"
    );
    assert!(doc.contains("**TL;DR**"), "entries carry their TL;DR");
    assert!(
        !doc.contains("### code-ranker — AI agent skill"),
        "AI.md excludes itself from its own index"
    );
}

#[test]
fn resolve_doc_resolves_base_doc_by_filename_stem() {
    // Docs that are neither a principle nor a node attribute resolve by their base
    // filename stem: hyphenated metric files (key is `fan_in`, file is `Fan-in`)
    // and the `metrics` reference.
    let s = snap(vec![], BTreeMap::new());
    assert_eq!(
        resolve_doc(&s, &TemplatesConfig::default(), "Fan-in").unwrap(),
        corpus_doc("base/Fan-in.md").unwrap()
    );
    assert_eq!(
        resolve_doc(&s, &TemplatesConfig::default(), "metrics").unwrap(),
        corpus_doc("base/metrics.md").unwrap()
    );
}

#[test]
fn doc_summary_prefers_tldr_then_first_paragraph() {
    let with_tldr = "# T\n\n**TL;DR**: line one\nline two\n\n## Next\nbody";
    assert_eq!(
        doc_summary(with_tldr).as_deref(),
        Some("**TL;DR**: line one line two")
    );
    let no_tldr = "# T\n\nFirst prose paragraph.\nstill it.\n\n## Next";
    assert_eq!(
        doc_summary(no_tldr).as_deref(),
        Some("First prose paragraph. still it.")
    );
}
