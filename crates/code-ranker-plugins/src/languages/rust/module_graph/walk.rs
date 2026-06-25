//! Module-tree walk cluster, extracted from `module_graph.rs` to keep per-file
//! complexity under the project's thresholds. Pure code movement: walks a
//! crate's files and inline modules, building module nodes / `contains` edges
//! and collecting pending `use` / bare-path references for later resolution.

use super::resolve::collect_use_paths;
use super::shared::{PendingUse, crate_label, module_node_id, target_kind_label};
use super::visitors::{
    CratePathCollector, FactsCollector, UnsafeCounter, convert_visibility, joined, mod_path_attr,
    resolve_submodule_path,
};
use crate::languages::rust::internal::{
    Edge, EdgeKind, GraphBuilder, Node, NodeId, NodeKind, Visibility,
};
use anyhow::{Context, Result};
use cargo_metadata::{Package, Target};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use syn::spanned::Spanned as _;
use syn::{Item, ItemMod};
// Re-exported into this module's scope so the `#[path]`-included `walk_tests`
// (which does `use super::*`) can name `SynVis` unchanged after the visibility
// helpers moved to `visitors.rs`.
#[cfg(test)]
use syn::Visibility as SynVis;

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
    let mut facts = FactsCollector::default();
    for item in &parsed.items {
        if ignore_tests && is_test_item(item) {
            continue;
        }
        syn::visit::Visit::visit_item(&mut collector, item);
        syn::visit::Visit::visit_item(&mut unsafe_counter, item);
        syn::visit::Visit::visit_item(&mut facts, item);
    }

    // `imports` = the qualified paths (≥2 segments) the file references — reuse
    // the bare-path collector that already drives the dependency edges.
    let imports: std::collections::BTreeSet<String> =
        collector.paths.iter().map(|segs| segs.join("::")).collect();

    // Annotate the parent module node with LOC, item_count, unsafe count + facts.
    if let Some(node) = builder
        .nodes_mut()
        .iter_mut()
        .find(|n| n.id == *parent_mod_id)
    {
        node.loc = Some(loc);
        node.item_count = Some(item_count);
        node.unsafe_count = Some(unsafe_counter.count);
        node.path = file_path.display().to_string();
        node.facts = super::super::internal::Facts {
            derives: joined(&facts.derives),
            macros: joined(&facts.macros),
            attrs: joined(&facts.attrs),
            imports: joined(&imports),
            types: joined(&facts.types),
            traits: joined(&facts.traits),
        };
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
        facts: Default::default(),
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
    // Shared with the metric test-stripper (`rust/test_attr.rs`) so the graph and
    // the metrics never disagree on what is test.
    attrs
        .iter()
        .any(crate::languages::rust::test_attr::is_test_attr)
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
