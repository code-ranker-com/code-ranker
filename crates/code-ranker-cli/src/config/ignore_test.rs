use super::*;
use serde_json::json;

/// A minimal `cargo metadata --format-version 1` shape: `packages` (id →
/// name), `workspace_members`, and `resolve.nodes` (id → deps, each with the
/// `dep_kinds` cargo actually emits — `[{"kind": null}]` for a normal edge).
fn metadata(workspace_members: &[&str], nodes: &[(&str, &[(&str, bool)])]) -> serde_json::Value {
    let mut names = std::collections::BTreeSet::new();
    names.extend(workspace_members.iter().copied());
    for (id, deps) in nodes {
        names.insert(*id);
        for (dep_id, _) in *deps {
            names.insert(*dep_id);
        }
    }
    let packages: Vec<_> = names
        .iter()
        .map(|id| json!({"id": id, "name": id}))
        .collect();
    let resolve_nodes: Vec<_> = nodes
        .iter()
        .map(|(id, deps)| {
            let deps_json: Vec<_> = deps
                .iter()
                .map(|(dep_id, dev_only)| {
                    let kind = if *dev_only { json!("dev") } else { json!(null) };
                    json!({"pkg": dep_id, "dep_kinds": [{"kind": kind}]})
                })
                .collect();
            json!({"id": id, "deps": deps_json})
        })
        .collect();
    json!({
        "packages": packages,
        "workspace_members": workspace_members,
        "resolve": {"nodes": resolve_nodes},
    })
}

/// A crate reached from a workspace member ONLY via a `dev` edge — never via
/// a regular one — is dev-only.
#[test]
fn dev_only_crates_from_metadata_finds_transitive_dev_only_dep() {
    let meta = metadata(
        &["root"],
        &[
            ("root", &[("regular_dep", false), ("dev_dep", true)]),
            ("regular_dep", &[]),
            ("dev_dep", &[]),
        ],
    );
    let dev_only = dev_only_crates_from_metadata(&meta);
    assert_eq!(dev_only, ["dev_dep".to_string()].into_iter().collect());
}

/// The same crate reached via a `dev` edge from one workspace member and a
/// regular edge from another is NOT dev-only — any real edge makes it
/// regular for the whole graph.
#[test]
fn dev_only_crates_from_metadata_excludes_dep_also_used_regularly() {
    let meta = metadata(
        &["a", "b"],
        &[
            ("a", &[("shared", true)]),
            ("b", &[("shared", false)]),
            ("shared", &[]),
        ],
    );
    let dev_only = dev_only_crates_from_metadata(&meta);
    assert!(
        dev_only.is_empty(),
        "a dep with any regular edge is not dev-only: {dev_only:?}"
    );
}

/// A workspace member itself is never reported as dev-only, even though
/// nothing else in the fixture points to it — it seeds `regular` directly.
#[test]
fn dev_only_crates_from_metadata_never_flags_a_workspace_member() {
    let meta = metadata(&["root"], &[("root", &[])]);
    let dev_only = dev_only_crates_from_metadata(&meta);
    assert!(dev_only.is_empty());
}

/// A `resolve.nodes` entry missing `id` or `deps` is skipped rather than
/// panicking — `cargo metadata` always includes both, but the graph walk
/// degrades gracefully around a malformed entry instead of indexing blindly,
/// and still resolves the rest of the (well-formed) graph correctly.
#[test]
fn dev_only_crates_from_metadata_skips_malformed_nodes() {
    let meta = json!({
        "packages": [
            {"id": "root", "name": "root"},
            {"id": "dev_dep", "name": "dev_dep"},
            {"id": "good_dep", "name": "good_dep"},
        ],
        "workspace_members": ["root"],
        "resolve": {"nodes": [
            // No `id` — the edges it carries can't be attributed to a source.
            {"deps": [{"pkg": "dev_dep", "dep_kinds": [{"kind": "dev"}]}]},
            // No `deps` — treated as a dead end, not a crash.
            {"id": "junk"},
            {"id": "root", "deps": [
                {"pkg": "dev_dep", "dep_kinds": [{"kind": "dev"}]},
                {"pkg": "good_dep", "dep_kinds": [{"kind": null}]},
            ]},
            {"id": "dev_dep", "deps": []},
            {"id": "good_dep", "deps": []},
        ]},
    });
    let dev_only = dev_only_crates_from_metadata(&meta);
    assert_eq!(
        dev_only,
        ["dev_dep".to_string()].into_iter().collect(),
        "the malformed nodes are skipped; the well-formed graph still resolves"
    );
}

fn file_node(id: &str, attrs: &[(&str, AttrValue)]) -> Node {
    let mut n = Node {
        id: id.into(),
        kind: "file".into(),
        name: id.into(),
        parent: None,
        attrs: Default::default(),
    };
    for (k, v) in attrs {
        n.attrs.insert((*k).into(), v.clone());
    }
    n
}

#[test]
fn strip_root_prefix_token_and_external() {
    assert_eq!(strip_root_prefix("{target}/src/a.rs"), "src/a.rs");
    assert_eq!(strip_root_prefix("ext:serde"), "ext:serde");
    assert_eq!(strip_root_prefix("plain/path.rs"), "plain/path.rs");
}

#[test]
fn build_glob_set_rejects_invalid_pattern() {
    assert!(build_glob_set(&["generated/**".into()]).is_ok());
    assert!(build_glob_set(&["a[".into()]).is_err());
}

#[test]
fn apply_ignore_strips_glob_matches_and_their_edges() {
    let mut g = Graph {
        nodes: vec![
            file_node("{target}/src/keep.rs", &[]),
            file_node("{target}/generated/gen.rs", &[]),
        ],
        edges: vec![code_ranker_plugin_api::edge::Edge {
            source: "{target}/src/keep.rs".into(),
            target: "{target}/generated/gen.rs".into(),
            kind: "uses".into(),
            line: None,
            attrs: Default::default(),
        }],
    };
    let ignore = IgnoreConfig {
        paths: vec!["generated/**".into()],
        ..Default::default()
    };
    let removed = apply_ignore(&mut g, &ignore, Path::new("/x")).unwrap();
    assert_eq!(removed, 1);
    assert_eq!(g.nodes.len(), 1);
    assert!(g.edges.is_empty(), "edge into a removed node is dropped");
}

/// `filter_graph` prunes external nodes whose crate is in the dev-only set
/// (matched by the `ext:<name>[@version]` base), keeps the rest. Drives the
/// dev-only branch directly with an in-memory set — no `cargo metadata`.
#[test]
fn filter_graph_drops_dev_only_external_crates() {
    let ext = |id: &str| file_node(id, &[("external", AttrValue::Bool(true))]);
    let mut g = Graph {
        nodes: vec![
            file_node("{target}/src/a.rs", &[]),
            ext("ext:devdep@2.0"),
            ext("ext:realdep"),
        ],
        edges: vec![],
    };
    let dev_only: HashSet<String> = ["devdep".to_string()].into_iter().collect();
    let removed = filter_graph(&mut g, None, &dev_only);
    assert_eq!(removed, 1, "only the dev-only external is dropped");
    assert!(
        g.nodes.iter().any(|n| n.id == "ext:realdep"),
        "a non-dev external is kept"
    );
    assert!(
        !g.nodes.iter().any(|n| n.id == "ext:devdep@2.0"),
        "the dev-only external (matched by its base name) is dropped"
    );
}

/// With nothing matching, `filter_graph` removes nothing and returns 0 (the
/// `removed.is_empty()` early return).
#[test]
fn filter_graph_removes_nothing_when_no_match() {
    let mut g = Graph {
        nodes: vec![file_node(
            "ext:keep",
            &[("external", AttrValue::Bool(true))],
        )],
        edges: vec![],
    };
    assert_eq!(filter_graph(&mut g, None, &HashSet::new()), 0);
    assert_eq!(g.nodes.len(), 1);
}
