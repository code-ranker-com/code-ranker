use super::*;
use crate::languages::rust::internal::{
    Edge, EdgeKind, Facts, InternalGraph, Node, NodeKind, Visibility,
};

/// A file-backed (`line = None`) or inline (`line = Some`) module node.
fn module(id: &str, path: &str, line: Option<u32>) -> Node {
    Node {
        id: id.into(),
        kind: NodeKind::Module,
        name: id.into(),
        path: path.into(),
        parent: None,
        external: None,
        version: None,
        visibility: Some(Visibility::Public),
        loc: Some(12),
        line,
        item_count: Some(3),
        unsafe_count: Some(1),
        crate_label: Some("demo".into()),
        facts: Facts {
            derives: Some("Serialize".into()),
            ..Facts::default()
        },
    }
}

fn krate(id: &str, path: &str, external: bool, version: Option<&str>) -> Node {
    Node {
        id: id.into(),
        kind: NodeKind::Crate,
        name: id.rsplit(':').next().unwrap_or(id).into(),
        path: path.into(),
        parent: None,
        external: Some(external),
        version: version.map(Into::into),
        visibility: None,
        loc: None,
        line: None,
        item_count: None,
        unsafe_count: None,
        crate_label: None,
        facts: Facts::default(),
    }
}

fn edge(from: &str, to: &str, kind: EdgeKind) -> Edge {
    Edge {
        from: from.into(),
        to: to.into(),
        kind,
        visibility: None,
        line: Some(1),
    }
}

/// `collapse_to_files` folds modules into file nodes (keyed by absolute path),
/// turns external crates into one `ext:` node each, re-points edges to file/
/// external granularity, drops crate→crate and self edges, and maps a local
/// crate to its root file.
#[test]
fn collapses_modules_externals_and_edges_to_file_level() {
    let mut g = InternalGraph::default();
    g.nodes
        .push(krate("crate:demo", "/p/Cargo.toml", false, None));
    g.nodes.push(module("mod:root", "/p/src/lib.rs", None));
    g.nodes.push(module("mod:b", "/p/src/b.rs", None));
    g.nodes.push(krate(
        "crate:serde",
        "/reg/serde-1.0.228/Cargo.toml",
        true,
        Some("1.0.228"),
    ));
    // crate owns its root file; lib.rs uses b.rs; b.rs uses serde; a crate→crate
    // dependency edge that must be dropped.
    g.edges
        .push(edge("crate:demo", "mod:root", EdgeKind::Contains));
    g.edges.push(edge("mod:root", "mod:b", EdgeKind::Uses));
    g.edges.push(edge("mod:b", "crate:serde", EdgeKind::Uses));
    g.edges
        .push(edge("crate:demo", "crate:serde", EdgeKind::Uses));

    let out = collapse_to_files(g);

    // Two file nodes + the referenced external; sorted by id.
    let ids: Vec<&str> = out.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(
        ids.contains(&"/p/src/lib.rs") && ids.contains(&"/p/src/b.rs"),
        "files: {ids:?}"
    );
    let ext = out
        .nodes
        .iter()
        .find(|n| n.kind == code_ranker_plugin_api::node::EXTERNAL)
        .expect("external node present");
    assert_eq!(ext.name, "serde");
    assert!(matches!(ext.attrs.get("version"), Some(AttrValue::Str(v)) if v == "1.0.228"));

    // The file-backed module is the source of truth for the file's structural attrs.
    let lib = out.nodes.iter().find(|n| n.id == "/p/src/lib.rs").unwrap();
    assert_eq!(lib.name, "lib.rs");
    assert!(matches!(lib.attrs.get("items"), Some(AttrValue::Int(3))));
    assert!(
        matches!(lib.attrs.get("unsafe"), Some(AttrValue::Int(1))),
        "unsafe>0 emitted"
    );
    assert!(lib.attrs.contains_key("derives"), "facts emitted");

    // Edges: lib→b (uses) and b→ext:serde (uses); the two crate-level edges dropped.
    let e: Vec<(&str, &str, &str)> = out
        .edges
        .iter()
        .map(|e| (e.source.as_str(), e.target.as_str(), e.kind.as_str()))
        .collect();
    assert!(
        e.contains(&("/p/src/lib.rs", "/p/src/b.rs", "uses")),
        "edges: {e:?}"
    );
    assert!(
        e.iter()
            .any(|(s, t, k)| *s == "/p/src/b.rs" && t.starts_with("ext:") && *k == "uses"),
        "edges: {e:?}"
    );
    assert_eq!(
        out.edges.len(),
        2,
        "crate→crate and self edges dropped: {e:?}"
    );
}

/// An inline module (`line = Some`) merges into the same file node without
/// overwriting the file-backed module's attrs; a `reexports` edge carries its
/// visibility attribute through.
#[test]
fn inline_module_merges_and_reexport_keeps_visibility() {
    let mut g = InternalGraph::default();
    g.nodes.push(module("mod:root", "/p/src/lib.rs", None)); // file-backed
    g.nodes
        .push(module("mod:inline", "/p/src/lib.rs", Some(40))); // inline, same file
    g.nodes.push(module("mod:b", "/p/src/b.rs", None));
    let mut re = edge("mod:root", "mod:b", EdgeKind::Reexports);
    re.visibility = Some(Visibility::Public);
    g.edges.push(re);

    let out = collapse_to_files(g);

    // Both modules collapsed into one /p/src/lib.rs node (no duplicate).
    assert_eq!(
        out.nodes.iter().filter(|n| n.id == "/p/src/lib.rs").count(),
        1
    );
    let reexport = out
        .edges
        .iter()
        .find(|e| e.kind == "reexports")
        .expect("reexport edge");
    assert!(
        matches!(reexport.attrs.get("visibility"), Some(AttrValue::Str(v)) if v == "public"),
        "reexport carries visibility"
    );
}
