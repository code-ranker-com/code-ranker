//! Module-tree walk cluster, extracted from `module_graph.rs` to keep per-file
//! complexity under the project's thresholds. Pure code movement: walks a
//! crate's files and inline modules, building module nodes / `contains` edges
//! and collecting pending `use` / bare-path references for later resolution.

use super::resolve::collect_use_paths;
use super::shared::{PendingUse, crate_label, module_node_id, target_kind_label};
use crate::languages::rust::internal::{
    Edge, EdgeKind, GraphBuilder, Node, NodeId, NodeKind, Visibility,
};
use anyhow::{Context, Result};
use cargo_metadata::{Package, Target};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use syn::spanned::Spanned as _;
use syn::{Item, ItemMod, Visibility as SynVis};

/// Collects every qualified path (≥ 2 segments) in a parsed file.
#[derive(Default)]
pub(super) struct CratePathCollector {
    pub(super) paths: std::collections::BTreeSet<Vec<String>>,
}

impl<'ast> syn::visit::Visit<'ast> for CratePathCollector {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if path.segments.len() >= 2 {
            self.paths
                .insert(path.segments.iter().map(|s| s.ident.to_string()).collect());
        }
        syn::visit::visit_path(self, path);
    }

    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        // `#[derive(...)]` arguments are an opaque token stream that the default
        // traversal never parses into paths, so a crate used *only* via a
        // qualified derive (e.g. `#[derive(serde::Serialize)]` with no `use
        // serde`) would otherwise produce no edge. Parse the derive list as a
        // comma-separated path list and record each qualified path.
        if attr
            .path()
            .is_ident(crate::languages::rust::cfg::SYN_DERIVE.as_str())
            && let Ok(paths) = attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            )
        {
            for p in &paths {
                if p.segments.len() >= 2 {
                    self.paths
                        .insert(p.segments.iter().map(|s| s.ident.to_string()).collect());
                }
            }
        }
        // Other attributes (`#[tokio::main]`, `#[serde(...)]`, …) keep the
        // default visit, which already routes the attribute's own path through
        // `visit_path`.
        syn::visit::visit_attribute(self, attr);
    }
}

/// Counts `unsafe` usages in a parsed file: `unsafe { }` expression blocks plus
/// `unsafe fn` / `unsafe impl` / `unsafe trait` declarations. Purely syntactic —
/// it does not (and cannot, without type info) tell an `unsafe` block doing real
/// work from a trivially-justified one, and `unsafe` produced inside a macro body
/// is invisible (macros are never expanded).
#[derive(Default)]
pub(super) struct UnsafeCounter {
    pub(super) count: u32,
}

impl<'ast> syn::visit::Visit<'ast> for UnsafeCounter {
    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        self.count += 1;
        syn::visit::visit_expr_unsafe(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if node.sig.unsafety.is_some() {
            self.count += 1;
        }
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if node.sig.unsafety.is_some() {
            self.count += 1;
        }
        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        if node.sig.unsafety.is_some() {
            self.count += 1;
        }
        syn::visit::visit_trait_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if node.unsafety.is_some() {
            self.count += 1;
        }
        syn::visit::visit_item_impl(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        if node.unsafety.is_some() {
            self.count += 1;
        }
        syn::visit::visit_item_trait(self, node);
    }
}

fn convert_visibility(v: &SynVis) -> Visibility {
    match v {
        SynVis::Public(_) => Visibility::Public,
        SynVis::Restricted(r) => {
            let s = r
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            // Path keywords are DATA (`[path_keywords]`); the empty path (a bare
            // `pub(in)` / inherited) is a syntax case, kept as `is_empty()`.
            let ss = s.as_str();
            if ss == crate::languages::rust::cfg::PK_CRATE.as_str() {
                Visibility::Crate
            } else if ss == crate::languages::rust::cfg::PK_SUPER.as_str() {
                Visibility::Super
            } else if ss == crate::languages::rust::cfg::PK_SELF.as_str() || ss.is_empty() {
                Visibility::Private
            } else {
                Visibility::Restricted { path: s }
            }
        }
        SynVis::Inherited => Visibility::Private,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn walk_file(
    file_path: &Path,
    parent_mod_id: &NodeId,
    parent_mod_path: &[String],
    pkg: &Package,
    target: &Target,
    ignore_tests: bool,
    module_index: &mut HashMap<Vec<String>, NodeId>,
    pending_uses: &mut Vec<PendingUse>,
    builder: &mut GraphBuilder,
    visited_files: &mut HashSet<PathBuf>,
) -> Result<()> {
    if !visited_files.insert(file_path.to_path_buf()) {
        return Ok(());
    }
    let content = std::fs::read_to_string(file_path)
        .with_context(|| format!("reading {}", file_path.display()))?;
    let parsed =
        syn::parse_file(&content).with_context(|| format!("parsing {}", file_path.display()))?;

    let loc = content.lines().count() as u32;
    let item_count = count_items(&parsed.items) as u32;

    // Walk the non-test items once, driving two visitors: the bare-path
    // collector and the `unsafe` counter. When skipping tests, visit only
    // non-test items so neither do references made solely by `#[cfg(test)]` code
    // become edges, nor does test-only `unsafe` inflate the count (consistent
    // with how `sloc`/complexity exclude tests).
    let mut collector = CratePathCollector::default();
    let mut unsafe_counter = UnsafeCounter::default();
    for item in &parsed.items {
        if ignore_tests && is_test_item(item) {
            continue;
        }
        syn::visit::Visit::visit_item(&mut collector, item);
        syn::visit::Visit::visit_item(&mut unsafe_counter, item);
    }

    // Annotate the parent module node with LOC, item_count and unsafe count.
    if let Some(node) = builder
        .nodes_mut()
        .iter_mut()
        .find(|n| n.id == *parent_mod_id)
    {
        node.loc = Some(loc);
        node.item_count = Some(item_count);
        node.unsafe_count = Some(unsafe_counter.count);
        node.path = file_path.display().to_string();
    }

    for path in collector.paths {
        pending_uses.push(PendingUse {
            from_mod_id: parent_mod_id.clone(),
            current_path: parent_mod_path.to_vec(),
            use_path: path,
            visibility: Visibility::Private,
            bare: true,
            glob: false,
            line: None,
        });
    }

    walk_items(
        &parsed.items,
        parent_mod_id,
        parent_mod_path,
        file_path,
        pkg,
        target,
        ignore_tests,
        module_index,
        pending_uses,
        builder,
        visited_files,
    )
}

#[allow(clippy::too_many_arguments)]
fn walk_items(
    items: &[Item],
    current_mod_id: &NodeId,
    current_mod_path: &[String],
    enclosing_file: &Path,
    pkg: &Package,
    target: &Target,
    ignore_tests: bool,
    module_index: &mut HashMap<Vec<String>, NodeId>,
    pending_uses: &mut Vec<PendingUse>,
    builder: &mut GraphBuilder,
    visited_files: &mut HashSet<PathBuf>,
) -> Result<()> {
    for item in items {
        // Skip `#[cfg(test)]` / `#[test]` / `#[bench]` items entirely when
        // requested — their modules, `use`s and bare paths are test-only.
        if ignore_tests && is_test_item(item) {
            continue;
        }
        match item {
            Item::Mod(m) => {
                process_mod(
                    m,
                    current_mod_id,
                    current_mod_path,
                    enclosing_file,
                    pkg,
                    target,
                    ignore_tests,
                    module_index,
                    pending_uses,
                    builder,
                    visited_files,
                )?;
            }
            Item::Use(u) => {
                let mut paths = Vec::new();
                collect_use_paths(&u.tree, Vec::new(), &mut paths);
                let vis = convert_visibility(&u.vis);
                let line = Some(u.span().start().line as u32);
                for (use_path, glob) in paths {
                    pending_uses.push(PendingUse {
                        from_mod_id: current_mod_id.clone(),
                        current_path: current_mod_path.to_vec(),
                        use_path,
                        visibility: vis.clone(),
                        bare: false,
                        glob,
                        line,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_mod(
    m: &ItemMod,
    parent_mod_id: &NodeId,
    parent_mod_path: &[String],
    enclosing_file: &Path,
    pkg: &Package,
    target: &Target,
    ignore_tests: bool,
    module_index: &mut HashMap<Vec<String>, NodeId>,
    pending_uses: &mut Vec<PendingUse>,
    builder: &mut GraphBuilder,
    visited_files: &mut HashSet<PathBuf>,
) -> Result<()> {
    let sub_name = m.ident.to_string();
    let mut sub_path = parent_mod_path.to_vec();
    sub_path.push(sub_name.clone());
    let sub_mod_id = module_node_id(
        &pkg.id.repr,
        target_kind_label(target),
        &target.name,
        &sub_path,
    );

    let (loc, line) = if m.content.is_some() {
        let span = m.span();
        let start = span.start().line as u32;
        let end = span.end().line as u32;
        (Some(end - start + 1), Some(start))
    } else {
        (None, None)
    };
    builder.add_node(Node {
        id: sub_mod_id.clone(),
        kind: NodeKind::Module,
        name: sub_name.clone(),
        path: enclosing_file.display().to_string(),
        parent: Some(parent_mod_id.clone()),
        external: None,
        version: None,
        visibility: Some(convert_visibility(&m.vis)),
        loc,
        line,
        item_count: None,
        unsafe_count: None,
        crate_label: Some(crate_label(pkg, target)),
    });
    builder.add_edge(Edge {
        from: parent_mod_id.clone(),
        to: sub_mod_id.clone(),
        kind: EdgeKind::Contains,
        visibility: None,
        line: None,
    });
    module_index.insert(sub_path.clone(), sub_mod_id.clone());

    if let Some((_, items)) = &m.content {
        walk_items(
            items,
            &sub_mod_id,
            &sub_path,
            enclosing_file,
            pkg,
            target,
            ignore_tests,
            module_index,
            pending_uses,
            builder,
            visited_files,
        )?;
    } else if let Some(sub_file) = mod_file_path(m, enclosing_file, &sub_name) {
        walk_file(
            &sub_file,
            &sub_mod_id,
            &sub_path,
            pkg,
            target,
            ignore_tests,
            module_index,
            pending_uses,
            builder,
            visited_files,
        )?;
    }
    Ok(())
}

/// Resolve the file backing `mod <name>;`. Honours an explicit
/// `#[path = "rel/or/abs.rs"]` attribute (relative to the directory of the file
/// containing the declaration) before falling back to the default
/// `name.rs` / `name/mod.rs` lookup. Without this, a `#[path]` module — and
/// every edge inside it — would be silently dropped.
fn mod_file_path(m: &ItemMod, enclosing_file: &Path, sub_name: &str) -> Option<PathBuf> {
    if let Some(rel) = mod_path_attr(m) {
        let base = enclosing_file.parent().unwrap_or_else(|| Path::new(""));
        let candidate = base.join(&rel);
        return candidate.exists().then_some(candidate);
    }
    resolve_submodule_path(enclosing_file, sub_name)
}

/// Read the string value of a `#[path = "..."]` attribute on a module, if present.
fn mod_path_attr(m: &ItemMod) -> Option<String> {
    for attr in &m.attrs {
        if attr
            .path()
            .is_ident(crate::languages::rust::cfg::SYN_PATH.as_str())
            && let syn::Meta::NameValue(nv) = &attr.meta
            && let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
        {
            return Some(s.value());
        }
    }
    None
}

fn resolve_submodule_path(parent_file: &Path, mod_name: &str) -> Option<PathBuf> {
    let parent_dir = parent_file.parent()?;
    let parent_stem = parent_file.file_stem()?.to_str()?;

    // Module-root stems and the source-file / dir-module conventions are DATA
    // (`module_roots` / `source_ext` / `dir_module_file` in `rust/config.toml`);
    // the lookup LOGIC stays here.
    let search_dir = if crate::languages::rust::cfg::MODULE_ROOTS
        .iter()
        .any(|s| s == parent_stem)
    {
        parent_dir.to_path_buf()
    } else {
        parent_dir.join(parent_stem)
    };

    let candidate_a = search_dir.join(format!(
        "{mod_name}.{}",
        crate::languages::rust::cfg::SOURCE_EXT.as_str()
    ));
    if candidate_a.exists() {
        return Some(candidate_a);
    }
    let candidate_b = search_dir
        .join(mod_name)
        .join(crate::languages::rust::cfg::DIR_MODULE_FILE.as_str());
    if candidate_b.exists() {
        return Some(candidate_b);
    }
    None
}

/// True for a top-level item gated to tests (`#[cfg(test)]` module,
/// `#[test]`/`#[bench]`/`#[cfg(test)]` fn, etc). Mirrors the line-stripping in
/// `code-ranker-complexity` so the graph and the metrics agree on what is test.
pub(super) fn is_test_item(item: &Item) -> bool {
    let attrs: &[syn::Attribute] = match item {
        Item::Mod(i) => &i.attrs,
        Item::Fn(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        Item::Struct(i) => &i.attrs,
        Item::Enum(i) => &i.attrs,
        Item::Trait(i) => &i.attrs,
        Item::Type(i) => &i.attrs,
        Item::Const(i) => &i.attrs,
        Item::Static(i) => &i.attrs,
        Item::Use(i) => &i.attrs,
        Item::Macro(i) => &i.attrs,
        Item::Union(i) => &i.attrs,
        _ => return false,
    };
    attrs.iter().any(is_test_attr)
}

/// True if an attribute gates an item to tests: `#[test]`, `#[bench]`, or a
/// `cfg(...)` whose predicate contains a bare `test` identifier
/// (`#[cfg(test)]`, `#[cfg(all(test, …))]`). `cfg(feature = "test")` does not
/// match — only the `test` *identifier* does.
fn is_test_attr(attr: &syn::Attribute) -> bool {
    // The `test` / `bench` / `cfg` attribute idents are DATA (`[syn]`).
    if attr
        .path()
        .is_ident(crate::languages::rust::cfg::SYN_TEST.as_str())
        || attr
            .path()
            .is_ident(crate::languages::rust::cfg::SYN_BENCH.as_str())
    {
        return true;
    }
    if attr
        .path()
        .is_ident(crate::languages::rust::cfg::SYN_CFG.as_str())
        && let Ok(list) = attr.meta.require_list()
    {
        return tokens_have_test_ident(list.tokens.clone());
    }
    false
}

/// Recursively scan a token stream for a bare `test` identifier (descends into
/// `all(...)` / `any(...)` / `not(...)` groups).
fn tokens_have_test_ident(ts: proc_macro2::TokenStream) -> bool {
    ts.into_iter().any(|tt| match tt {
        proc_macro2::TokenTree::Ident(i) => i == crate::languages::rust::cfg::SYN_TEST.as_str(),
        proc_macro2::TokenTree::Group(g) => tokens_have_test_ident(g.stream()),
        _ => false,
    })
}

fn count_items(items: &[Item]) -> usize {
    items
        .iter()
        .filter(|i| {
            matches!(
                i,
                Item::Fn(_)
                    | Item::Struct(_)
                    | Item::Enum(_)
                    | Item::Trait(_)
                    | Item::Impl(_)
                    | Item::Type(_)
                    | Item::Const(_)
                    | Item::Static(_)
                    | Item::Mod(_)
                    | Item::Macro(_)
                    | Item::Union(_)
            )
        })
        .count()
}

#[cfg(test)]
#[path = "../tests/walk.rs"]
mod walk_tests;
