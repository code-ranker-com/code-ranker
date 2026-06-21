//! The Python dependency-graph (structure) builder.
//!
//! Imperative-only code: walk the workspace for `.py` files, map each to its
//! dotted module path, parse imports with `tree-sitter-python`, and resolve them
//! to `uses` edges between file nodes (with one `external` node per unresolved
//! top-level package). The thin `LanguagePlugin::analyze` in `mod.rs` calls
//! [`analyze`]; everything below is the machinery it drives.

use anyhow::Result;
use code_ranker_plugin_api::{attrs::AttrValue, edge::Edge, graph::Graph, node::Node};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

// The file→module-path mapping (`MODULE` + `file_to_module_path`), the workspace
// module index, and the import resolver live in a cohesive child submodule
// (`imports.rs`) that depends only on `crate::config` downward — never back on
// `structure` — so the module graph stays acyclic. Re-import the moved items so
// every call site below (and the `use super::*` tests) compiles exactly as before.
#[path = "imports.rs"]
mod imports;
#[cfg(test)]
use imports::absolute_base;
use imports::{build_module_index, file_to_module_path, resolve_import};

/// Import-graph tree-sitter NODE-KIND strings the walk keys on, plus the
/// file-collection / skip-dir DATA lists, resolved once from `python/config.toml`.
/// Loaded here (self-contained, depending only on `crate::config`) rather than
/// via the parent `mod.rs`'s `CONFIG` to keep the Python module graph acyclic.
/// The walk LOGIC stays in Rust; *which* node kinds it matches, *which*
/// extensions it collects, and *which* directory names it prunes are data.
struct StructureKinds {
    import_statement: String,
    import_from_statement: String,
    dotted_name: String,
    aliased_import: String,
    /// Source-file extensions the walk collects (`["py"]`).
    extensions: Vec<String>,
    /// Directory names pruned by exact match during the walk (the leading-`.`
    /// rule is a separate syntax rule kept in `is_skip_path`).
    skip_dirs: Vec<String>,
    /// Test-path convention DATA the `py_is_test_path` predicate keys on (the
    /// predicate LOGIC stays in Rust). `test_dirs`: a path component matched
    /// exactly; `test_files`: a filename matched exactly; `test_prefixes`: a
    /// `.py` filename matched via `starts_with`; `test_suffixes`: a filename
    /// matched via `ends_with`.
    test_dirs: Vec<String>,
    test_files: Vec<String>,
    test_prefixes: Vec<String>,
    test_suffixes: Vec<String>,
    /// `ext:` node-id namespace prefix for an external node (`[ids].external`,
    /// inherited from `defaults.toml`).
    ext_prefix: String,
    /// Source extensions with a leading dot (`[".py"]`), derived from
    /// `extensions` — the suffix `py_is_test_path` gates on. Not re-spelled in
    /// the config. (The file→module mapping's copy lives in `imports::MODULE`.)
    dot_exts: Vec<String>,
    /// tree-sitter field names the import walk navigates (`[fields]`).
    field_name: String,
    field_module_name: String,
    /// Visibility output strings (`[visibility]`): `public` inherited from
    /// `defaults.toml`, `restricted` / `private` from `python/config.toml`.
    vis_public: String,
    vis_restricted: String,
    vis_private: String,
}

static KINDS: LazyLock<StructureKinds> = LazyLock::new(|| {
    let cfg = crate::config::load(include_str!("config.toml"));
    let s = crate::config::string_table(&cfg, "structure");
    let get = |k: &str| s.get(k).cloned().expect("[structure] key");
    let extensions = crate::config::string_list(&cfg, "extensions");
    let dot_exts = extensions.iter().map(|e| format!(".{e}")).collect();
    let f = crate::config::string_table(&cfg, "fields");
    let field = |k: &str| f.get(k).cloned().expect("[fields] key");
    let vis = crate::config::string_table(&cfg, "visibility");
    let vis_get = |k: &str| vis.get(k).cloned().expect("[visibility] key");
    StructureKinds {
        import_statement: get("import_statement"),
        import_from_statement: get("import_from_statement"),
        dotted_name: get("dotted_name"),
        aliased_import: get("aliased_import"),
        extensions,
        skip_dirs: crate::config::string_list(&cfg, "skip_dirs"),
        test_dirs: crate::config::string_list(&cfg, "test_dirs"),
        test_files: crate::config::string_list(&cfg, "test_files"),
        test_prefixes: crate::config::string_list(&cfg, "test_prefixes"),
        test_suffixes: crate::config::string_list(&cfg, "test_suffixes"),
        ext_prefix: crate::config::string_table(&cfg, "ids")
            .get("external")
            .cloned()
            .expect("python [ids].external (inherited from defaults.toml)"),
        dot_exts,
        field_name: field("name"),
        field_module_name: field("module_name"),
        vis_public: vis_get("public"),
        vis_restricted: vis_get("restricted"),
        vis_private: vis_get("private"),
    }
});

/// The `uses` edge-kind identifier the structure builder tags `uses` edges with,
/// resolved via `config::edge_kind_id` against the merged `[edge_kinds]` (the same
/// table `mod.rs::levels()` publishes), so the tagged `kind` and the level
/// descriptor can never drift. Mirrors `rust/collapse.rs`'s pattern: the `"uses"`
/// lookup key is the variant slot, validated against the published vocabulary.
fn uses_edge_kind() -> &'static str {
    static USES: LazyLock<()> = LazyLock::new(|| {
        let cfg = crate::config::load(include_str!("config.toml"));
        crate::config::edge_kind_id(&cfg, "uses")
            .unwrap_or_else(|| panic!("python/config.toml [edge_kinds] is missing `uses`"));
    });
    LazyLock::force(&USES);
    "uses"
}

/// A node-attribute key, validated against `[node_attributes]` (inherited from
/// `defaults.toml`) so an inserted attr can never use an undeclared key. Mirrors
/// `uses_edge_kind`.
fn attr_key(key: &'static str) -> &'static str {
    static CFG: LazyLock<toml::Table> =
        LazyLock::new(|| crate::config::load(include_str!("config.toml")));
    crate::config::attr_key(&CFG, key)
        .unwrap_or_else(|| panic!("python [node_attributes] is missing `{key}`"));
    key
}

/// Python test conventions: pytest/unittest files (`test_*.py`, `*_test.py`,
/// `conftest.py`) and anything under a `tests/` directory.
pub(super) fn py_is_test_path(rel_path: &str) -> bool {
    let file = rel_path.rsplit('/').next().unwrap_or(rel_path);
    rel_path
        .split('/')
        .any(|c| KINDS.test_dirs.iter().any(|d| d == c))
        || KINDS.test_files.iter().any(|f| f == file)
        || (KINDS.dot_exts.iter().any(|e| file.ends_with(e.as_str()))
            && KINDS
                .test_prefixes
                .iter()
                .any(|p| file.starts_with(p.as_str())))
        || KINDS
            .test_suffixes
            .iter()
            .any(|s| file.ends_with(s.as_str()))
}

pub(super) fn analyze(
    workspace: &Path,
    ignore_tests: bool,
    ignore: &crate::config::IgnoreCfg,
) -> Result<Graph> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();

    let py_files = collect_py_files(workspace, ignore_tests, ignore);
    let module_index = build_module_index(workspace, &py_files);

    // Track external nodes already added (by id) to avoid duplicates.
    let mut ext_seen: HashSet<String> = HashSet::new();

    for abs_path in &py_files {
        let Some(mod_path) = file_to_module_path(workspace, abs_path) else {
            continue;
        };
        parse_and_add(
            abs_path,
            &mod_path,
            &module_index,
            &mut nodes,
            &mut edges,
            &mut ext_seen,
        )?;
    }

    Ok(Graph { nodes, edges })
}

// ---------------------------------------------------------------------------
// File discovery
// ---------------------------------------------------------------------------

fn collect_py_files(
    workspace: &Path,
    ignore_tests: bool,
    ignore: &crate::config::IgnoreCfg,
) -> Vec<PathBuf> {
    crate::walk::collect(workspace, &KINDS.skip_dirs, ignore, |p| {
        p.extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| KINDS.extensions.iter().any(|e| e == x))
    })
    .into_iter()
    .filter(|p| !(ignore_tests && is_test_file(p, workspace)))
    .collect()
}

/// Workspace-relative test check used during the walk.
fn is_test_file(path: &Path, workspace: &Path) -> bool {
    path.strip_prefix(workspace)
        .ok()
        .map(|rel| py_is_test_path(&rel.to_string_lossy().replace('\\', "/")))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Per-file parsing
// ---------------------------------------------------------------------------

struct ExtractedImport {
    base: String,       // "parser.shops.amazon" or ".." or ".utils"
    names: Vec<String>, // imported names; empty for plain `import X`
    line: u32,          // 1-based line of the import statement
}

fn parse_and_add(
    abs_path: &Path,
    mod_path: &str,
    module_index: &HashMap<String, PathBuf>,
    nodes: &mut Vec<Node>,
    edges: &mut Vec<Edge>,
    ext_seen: &mut HashSet<String>,
) -> Result<()> {
    let source = std::fs::read(abs_path)?;

    let mut ts_parser = tree_sitter::Parser::new();
    ts_parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let tree = ts_parser
        .parse(&source, None)
        .ok_or_else(|| anyhow::anyhow!("parse failed: {}", abs_path.display()))?;

    let loc = source.iter().filter(|&&b| b == b'\n').count() as i64 + 1;
    // NEW id scheme: plain absolute path (no "file:" prefix).
    let file_id = abs_path.to_string_lossy().into_owned();

    let parts: Vec<&str> = mod_path.split('.').collect();
    let vis_str = py_visibility_str(parts[parts.len() - 1]);

    let mut file_attrs = BTreeMap::new();
    file_attrs.insert(
        attr_key("visibility").to_string(),
        AttrValue::Str(vis_str.into()),
    );
    file_attrs.insert(attr_key("loc").to_string(), AttrValue::Int(loc));

    nodes.push(Node {
        id: file_id.clone(),
        kind: code_ranker_plugin_api::node::FILE.into(),
        name: abs_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        parent: None,
        attrs: file_attrs,
    });

    // Walk tree for imports only.
    let imports = extract_imports(&tree.root_node(), &source);

    for imp in &imports {
        let targets = resolve_import(&imp.base, &imp.names, mod_path, module_index);
        if targets.is_empty() {
            // Unresolved → external (3rd-party / stdlib). One External node per top-level package.
            add_external_edge(imp, &file_id, nodes, edges, ext_seen);
            continue;
        }
        for target_path in targets {
            let target_id = target_path.to_string_lossy().into_owned();
            if target_id != file_id {
                edges.push(Edge {
                    source: file_id.clone(),
                    target: target_id,
                    kind: uses_edge_kind().into(),
                    line: Some(imp.line),
                    attrs: BTreeMap::new(),
                });
            }
        }
    }

    Ok(())
}

/// Emit a `uses` edge from `file_id` to the external node for an unresolved
/// import, materialising the (deduplicated) External node on first sight. A
/// no-op for relative imports, which are never external libraries.
fn add_external_edge(
    imp: &ExtractedImport,
    file_id: &str,
    nodes: &mut Vec<Node>,
    edges: &mut Vec<Edge>,
    ext_seen: &mut HashSet<String>,
) {
    let Some(top) = external_top_level(&imp.base) else {
        return;
    };
    let ext_id = format!("{}{top}", KINDS.ext_prefix);
    if ext_seen.insert(ext_id.clone()) {
        let mut ext_attrs = BTreeMap::new();
        ext_attrs.insert(attr_key("external").to_string(), AttrValue::Bool(true));
        nodes.push(Node {
            id: ext_id.clone(),
            kind: code_ranker_plugin_api::node::EXTERNAL.into(),
            name: top,
            parent: None,
            attrs: ext_attrs,
        });
    }
    edges.push(Edge {
        source: file_id.to_string(),
        target: ext_id,
        kind: uses_edge_kind().into(),
        line: Some(imp.line),
        attrs: BTreeMap::new(),
    });
}

/// Top-level package name for an unresolved import, or `None` for relative
/// imports (which are always project-internal and never external libraries).
fn external_top_level(base: &str) -> Option<String> {
    if base.starts_with('.') || base.is_empty() {
        return None;
    }
    Some(base.split('.').next().unwrap_or(base).to_string())
}

// ---------------------------------------------------------------------------
// Tree-sitter extraction (imports only)
// ---------------------------------------------------------------------------

fn extract_imports(root: &tree_sitter::Node, source: &[u8]) -> Vec<ExtractedImport> {
    let mut imports = Vec::new();
    visit_imports(root, source, &mut imports);
    imports
}

fn visit_imports<'t>(
    node: &tree_sitter::Node<'t>,
    source: &[u8],
    imports: &mut Vec<ExtractedImport>,
) {
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node<'t>> = node.children(&mut cursor).collect();

    for child in &children {
        let kind = child.kind();
        if kind == KINDS.import_statement {
            // import a.b.c  OR  import a, b
            handle_import_statement(child, source, imports);
        } else if kind == KINDS.import_from_statement {
            handle_import_from_statement(child, source, imports);
        } else {
            visit_imports(child, source, imports);
        }
    }
}

/// Collect every dotted name from a plain `import a.b.c` / `import a, b`
/// statement, unwrapping `import x as y` aliases to their real name.
fn handle_import_statement(
    stmt: &tree_sitter::Node,
    source: &[u8],
    imports: &mut Vec<ExtractedImport>,
) {
    let line = stmt.start_position().row as u32 + 1;
    let mut ic = stmt.walk();
    for c in stmt.children(&mut ic) {
        let actual = if c.kind() == KINDS.aliased_import {
            c.child_by_field_name(&KINDS.field_name).unwrap_or(c)
        } else {
            c
        };
        if actual.kind() == KINDS.dotted_name
            && let Ok(t) = actual.utf8_text(source)
        {
            imports.push(ExtractedImport {
                base: t.to_string(),
                names: vec![],
                line,
            });
        }
    }
}

/// Collect the module base plus imported names from a `from M import a, b`
/// statement, skipping the module-name node itself and unwrapping aliases.
fn handle_import_from_statement(
    stmt: &tree_sitter::Node,
    source: &[u8],
    imports: &mut Vec<ExtractedImport>,
) {
    let base = stmt
        .child_by_field_name(&KINDS.field_module_name)
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("")
        .to_string();

    let mut names = Vec::new();
    let mut ic = stmt.walk();
    for c in stmt.children(&mut ic) {
        let actual = if c.kind() == KINDS.aliased_import {
            c.child_by_field_name(&KINDS.field_name).unwrap_or(c)
        } else {
            c
        };
        if actual.kind() == KINDS.dotted_name
            && actual.start_byte()
                != stmt
                    .child_by_field_name(&KINDS.field_module_name)
                    .map_or(0, |n| n.start_byte())
            && let Ok(t) = actual.utf8_text(source)
        {
            names.push(t.to_string());
        }
    }

    if !base.is_empty() {
        let line = stmt.start_position().row as u32 + 1;
        imports.push(ExtractedImport { base, names, line });
    }
}

// ---------------------------------------------------------------------------
// Visibility heuristic
// ---------------------------------------------------------------------------

fn py_visibility_str(name: &str) -> &'static str {
    // Output strings are DATA (`[visibility]`); the `_`/`__` naming-convention
    // LOGIC stays here as a Python syntax rule.
    if name.starts_with("__") && !name.ends_with("__") {
        &KINDS.vis_private
    } else if name.starts_with('_') {
        &KINDS.vis_restricted
    } else {
        &KINDS.vis_public
    }
}

#[cfg(test)]
#[path = "tests/structure.rs"]
mod structure_tests;
