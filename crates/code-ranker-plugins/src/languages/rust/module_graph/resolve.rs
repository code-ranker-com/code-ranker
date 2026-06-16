//! `use`-resolution cluster, extracted from `module_graph.rs` to keep per-file
//! complexity under the project's thresholds. Pure code movement: resolves
//! pending `use` / bare paths against the owning crate's module index, the
//! workspace library indexes (cross-crate), and the extern-crate map, then
//! emits the resulting edges.

use super::shared::{
    ForeignLib, MAX_REEXPORT_DEPTH, PendingUse, ReexportMap, build_reexports, is_reexport,
};
use crate::languages::rust::internal::{Edge, EdgeKind, GraphBuilder, NodeId};
use std::collections::{HashMap, HashSet};
use syn::UseTree;

/// Flatten a `use` tree to `(path, is_glob)` leaves; `is_glob` marks the `::*`
/// terminator so resolution can tell a namespace pull apart from a named import.
pub(super) fn collect_use_paths(
    tree: &UseTree,
    prefix: Vec<String>,
    out: &mut Vec<(Vec<String>, bool)>,
) {
    match tree {
        UseTree::Path(p) => {
            let mut new_prefix = prefix;
            new_prefix.push(p.ident.to_string());
            collect_use_paths(&p.tree, new_prefix, out);
        }
        UseTree::Name(n) => {
            let mut path = prefix;
            path.push(n.ident.to_string());
            out.push((path, false));
        }
        UseTree::Rename(r) => {
            let mut path = prefix;
            path.push(r.ident.to_string());
            out.push((path, false));
        }
        UseTree::Glob(_) => {
            if !prefix.is_empty() {
                out.push((prefix, true));
            }
        }
        UseTree::Group(g) => {
            for sub in &g.items {
                collect_use_paths(sub, prefix.clone(), out);
            }
        }
    }
}

/// Lexical module a glob `use` pulls from, resolved against the current module
/// path (`crate::a::b` → `[a,b]`, `super::*` → parent, `self::x` → child). Returns
/// `None` for a path that doesn't denote an in-crate module.
fn glob_target_module(use_path: &[String], current_path: &[String]) -> Option<Vec<String>> {
    // Path keywords (`crate`/`self`/`super`) are DATA (`[path_keywords]`); each
    // drives distinct logic, so this is an if-chain, not a list match.
    use crate::languages::rust::cfg::{PK_CRATE, PK_SELF, PK_SUPER};
    let first = use_path.first().map(String::as_str)?;
    if first == PK_CRATE.as_str() {
        Some(use_path[1..].to_vec())
    } else if first == PK_SELF.as_str() {
        let mut p = current_path.to_vec();
        p.extend_from_slice(&use_path[1..]);
        Some(p)
    } else if first == PK_SUPER.as_str() {
        let mut p = current_path.to_vec();
        let mut tail = use_path;
        while tail.first().map(String::as_str) == Some(PK_SUPER.as_str()) {
            p.pop()?;
            tail = &tail[1..];
        }
        p.extend_from_slice(tail);
        Some(p)
    } else {
        // Bare first segment in a `use`: crate-relative child module (2018) —
        // a descendant, never an ancestor.
        let mut p = current_path.to_vec();
        p.extend_from_slice(use_path);
        Some(p)
    }
}

/// True when a glob `use` pulls in a *strict ancestor* module's namespace
/// (`use super::*`, `use crate::<ancestor>::*`). This is structural scope-sugar
/// (the child reaching back into its enclosing module), not a real outward
/// dependency, so it is emitted as `EdgeKind::Super` rather than `Uses`.
pub(super) fn is_super_glob(pu: &PendingUse) -> bool {
    if !pu.glob {
        return false;
    }
    let Some(target) = glob_target_module(&pu.use_path, &pu.current_path) else {
        return false;
    };
    target.len() < pu.current_path.len() && pu.current_path[..target.len()] == target[..]
}

pub(super) fn emit_uses(
    pending: &[PendingUse],
    module_index: &HashMap<Vec<String>, NodeId>,
    extern_crates: &HashMap<String, NodeId>,
    dep_pkg_by_name: &HashMap<String, String>,
    lib_index: &HashMap<String, ForeignLib>,
    builder: &mut GraphBuilder,
) {
    let reexports = build_reexports(pending);
    let mut seen: HashSet<(NodeId, NodeId, String)> = HashSet::new();
    for pu in pending {
        let Some(target_id) = resolve_use_path(
            &pu.use_path,
            &pu.current_path,
            module_index,
            extern_crates,
            dep_pkg_by_name,
            lib_index,
            &reexports,
            0,
        ) else {
            continue;
        };
        if target_id == pu.from_mod_id {
            continue;
        }
        let kind = if !pu.bare && is_reexport(&pu.visibility) {
            EdgeKind::Reexports
        } else if is_super_glob(pu) {
            EdgeKind::Super
        } else {
            EdgeKind::Uses
        };
        let kind_str = format!("{kind:?}");
        if !seen.insert((pu.from_mod_id.clone(), target_id.clone(), kind_str)) {
            continue;
        }
        builder.add_edge(Edge {
            from: pu.from_mod_id.clone(),
            to: target_id,
            kind,
            visibility: if matches!(kind, EdgeKind::Reexports) {
                Some(pu.visibility.clone())
            } else {
                None
            },
            line: pu.line,
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_use_path(
    use_path: &[String],
    current_path: &[String],
    module_index: &HashMap<Vec<String>, NodeId>,
    extern_crates: &HashMap<String, NodeId>,
    dep_pkg_by_name: &HashMap<String, String>,
    lib_index: &HashMap<String, ForeignLib>,
    reexports: &ReexportMap,
    depth: usize,
) -> Option<NodeId> {
    if use_path.is_empty() {
        return None;
    }
    let first = use_path[0].as_str();
    let rest = &use_path[1..];

    // Path keywords (`crate`/`self`/`super`) and standard-distribution crate
    // roots (`std_crates`) are DATA (`[path_keywords]` / `std_crates`). Each path
    // keyword drives distinct resolution logic, so this is an if-chain, not a list
    // match; `std_crates` IS a uniform list (a `use` rooted there → no edge).
    use crate::languages::rust::cfg::{PK_CRATE, PK_SELF, PK_SUPER, STD_CRATES};
    if first == PK_CRATE.as_str() {
        resolve_in_index(
            &[],
            rest,
            module_index,
            extern_crates,
            dep_pkg_by_name,
            lib_index,
            reexports,
            depth,
        )
    } else if first == PK_SELF.as_str() {
        resolve_in_index(
            current_path,
            rest,
            module_index,
            extern_crates,
            dep_pkg_by_name,
            lib_index,
            reexports,
            depth,
        )
    } else if first == PK_SUPER.as_str() {
        let mut path = current_path.to_vec();
        let mut tail = rest;
        while tail.first().map(|s| s.as_str()) == Some(PK_SUPER.as_str()) {
            path.pop()?;
            tail = &tail[1..];
        }
        path.pop()?;
        resolve_in_index(
            &path,
            tail,
            module_index,
            extern_crates,
            dep_pkg_by_name,
            lib_index,
            reexports,
            depth,
        )
    } else if STD_CRATES.iter().any(|s| s == first) {
        None
    } else {
        let other = first;
        let mut probe = current_path.to_vec();
        probe.push(first.to_string());
        if module_index.contains_key(&probe) {
            return resolve_in_index(
                current_path,
                use_path,
                module_index,
                extern_crates,
                dep_pkg_by_name,
                lib_index,
                reexports,
                depth,
            );
        }
        // Cross-crate into another local workspace crate: walk the rest of
        // the path through that crate's library, following its `pub use`
        // re-exports so the edge lands on the file that owns the item
        // (a re-exported `other_crate::Symbol` → its defining file, not the
        // crate root; a path stopping at a non-module, non-re-exported item
        // still falls back to the crate root).
        if let Some(dep_repr) = dep_pkg_by_name.get(other)
            && let Some(foreign) = lib_index.get(dep_repr)
        {
            return walk_foreign(&[], rest, &foreign.index, &foreign.reexports, 0);
        }
        // Registry dependency (or a local crate with no library target):
        // collapse onto the crate root node.
        extern_crates.get(other).cloned()
    }
}

/// Walk `base ++ tail` through the module tree, returning the deepest matching
/// module node, the path that reached it, and how many `tail` segments were
/// consumed (a trailing item like a struct/fn leaves a leftover segment).
fn walk_detailed(
    base: &[String],
    tail: &[String],
    module_index: &HashMap<Vec<String>, NodeId>,
) -> Option<(NodeId, Vec<String>, usize)> {
    let mut cur = base.to_vec();
    let mut node = module_index.get(&cur)?.clone();
    let mut consumed = 0usize;
    for seg in tail {
        let mut probe = cur.clone();
        probe.push(seg.clone());
        match module_index.get(&probe) {
            Some(id) => {
                node = id.clone();
                cur = probe;
                consumed += 1;
            }
            None => break,
        }
    }
    Some((node, cur, consumed))
}

/// Resolve `base ++ tail` within a **foreign** crate's library, following its
/// `pub use` re-exports so a re-exported `other_crate::Symbol` lands on the file
/// that defines `Symbol` rather than the foreign crate root. Self-contained: it
/// consults only the foreign crate's index and re-export table (a foreign
/// re-export of a *third* crate is left at the foreign module — a rare,
/// acceptable degradation).
fn walk_foreign(
    base: &[String],
    tail: &[String],
    index: &HashMap<Vec<String>, NodeId>,
    reexports: &ReexportMap,
    depth: usize,
) -> Option<NodeId> {
    let (node, stop_path, consumed) = walk_detailed(base, tail, index)?;
    if consumed >= tail.len() {
        return Some(node);
    }
    if depth < MAX_REEXPORT_DEPTH
        && let Some(entries) = reexports.get(&stop_path)
    {
        let sym = &tail[consumed];
        for (exported, source) in entries {
            if exported == sym
                && let Some(redirected) =
                    resolve_foreign_source(source, &stop_path, index, reexports, depth + 1)
                && redirected != node
            {
                return Some(redirected);
            }
        }
    }
    Some(node)
}

/// Resolve a `pub use` source path *within* a foreign crate (handles
/// `crate` / `self` / `super` / submodule prefixes). Keyword/external paths
/// yield `None`, so the caller keeps the facade module.
fn resolve_foreign_source(
    use_path: &[String],
    current_path: &[String],
    index: &HashMap<Vec<String>, NodeId>,
    reexports: &ReexportMap,
    depth: usize,
) -> Option<NodeId> {
    if use_path.is_empty() {
        return None;
    }
    let first = use_path[0].as_str();
    let rest = &use_path[1..];
    // Path keywords / `std_crates` are DATA; see the note in `resolve_use_path`.
    use crate::languages::rust::cfg::{PK_CRATE, PK_SELF, PK_SUPER, STD_CRATES};
    if first == PK_CRATE.as_str() {
        walk_foreign(&[], rest, index, reexports, depth)
    } else if first == PK_SELF.as_str() {
        walk_foreign(current_path, rest, index, reexports, depth)
    } else if first == PK_SUPER.as_str() {
        let mut path = current_path.to_vec();
        let mut tail = rest;
        while tail.first().map(|s| s.as_str()) == Some(PK_SUPER.as_str()) {
            path.pop()?;
            tail = &tail[1..];
        }
        path.pop()?;
        walk_foreign(&path, tail, index, reexports, depth)
    } else if STD_CRATES.iter().any(|s| s == first) {
        None
    } else {
        let mut probe = current_path.to_vec();
        probe.push(first.to_string());
        if index.contains_key(&probe) {
            walk_foreign(current_path, use_path, index, reexports, depth)
        } else {
            None
        }
    }
}

/// Resolve a path within the owning crate's module tree, following `pub use`
/// re-exports for a trailing symbol so the edge lands on the file that *defines*
/// the symbol rather than a facade module that re-exports it.
#[allow(clippy::too_many_arguments)]
fn resolve_in_index(
    base: &[String],
    tail: &[String],
    module_index: &HashMap<Vec<String>, NodeId>,
    extern_crates: &HashMap<String, NodeId>,
    dep_pkg_by_name: &HashMap<String, String>,
    lib_index: &HashMap<String, ForeignLib>,
    reexports: &ReexportMap,
    depth: usize,
) -> Option<NodeId> {
    let (node, stop_path, consumed) = walk_detailed(base, tail, module_index)?;
    if consumed >= tail.len() {
        // Fully resolved to a module (e.g. `use crate::a::b` where `b` is a mod).
        return Some(node);
    }
    // A leftover segment is a non-module item (struct/fn/const/…). If the module
    // we stopped at re-exports it via `pub use`, follow that to the definer.
    if depth < MAX_REEXPORT_DEPTH
        && let Some(entries) = reexports.get(&stop_path)
    {
        let sym = &tail[consumed];
        for (exported, source) in entries {
            if exported != sym {
                continue;
            }
            if let Some(redirected) = resolve_use_path(
                source,
                &stop_path,
                module_index,
                extern_crates,
                dep_pkg_by_name,
                lib_index,
                reexports,
                depth + 1,
            ) && redirected != node
            {
                return Some(redirected);
            }
        }
    }
    Some(node)
}

#[cfg(test)]
#[path = "../tests/resolve.rs"]
mod resolve_tests;
