//! Visibility / attribute / module-path helpers plus the per-file `syn` fact
//! visitors, extracted from `walk.rs` to keep per-file complexity under the
//! project's thresholds. Pure code movement: these functions and visitors depend
//! only on their arguments plus shared/external types, so the relocation is
//! behaviour-preserving.

use crate::languages::rust::internal::Visibility;
use std::path::{Path, PathBuf};
use syn::{ItemMod, Visibility as SynVis};

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

/// Collects per-file syntactic facts for config `[rules.checks]`: derive names,
/// macro-invocation names, attribute names (non-derive), and the names of types
/// and traits defined in the file. Driven over production items only (test items
/// are skipped by the caller), so a `#[cfg(test)]`-only derive never counts.
#[derive(Default)]
pub(super) struct FactsCollector {
    pub(super) derives: std::collections::BTreeSet<String>,
    pub(super) macros: std::collections::BTreeSet<String>,
    pub(super) attrs: std::collections::BTreeSet<String>,
    pub(super) types: std::collections::BTreeSet<String>,
    pub(super) traits: std::collections::BTreeSet<String>,
}

impl<'ast> syn::visit::Visit<'ast> for FactsCollector {
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if attr
            .path()
            .is_ident(crate::languages::rust::cfg::SYN_DERIVE.as_str())
        {
            if let Ok(paths) = attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            ) {
                for p in &paths {
                    if let Some(seg) = p.segments.last() {
                        self.derives.insert(seg.ident.to_string());
                    }
                }
            }
        } else if let Some(seg) = attr.path().segments.last() {
            let name = seg.ident.to_string();
            // Skip ubiquitous noise attributes — they carry no rule signal.
            if !matches!(
                name.as_str(),
                "doc" | "allow" | "warn" | "deny" | "cfg" | "cfg_attr"
            ) {
                self.attrs.insert(name);
            }
        }
        syn::visit::visit_attribute(self, attr);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if let Some(seg) = mac.path.segments.last() {
            self.macros.insert(seg.ident.to_string());
        }
        syn::visit::visit_macro(self, mac);
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        self.types.insert(i.ident.to_string());
        syn::visit::visit_item_struct(self, i);
    }

    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        self.types.insert(i.ident.to_string());
        syn::visit::visit_item_enum(self, i);
    }

    fn visit_item_type(&mut self, i: &'ast syn::ItemType) {
        self.types.insert(i.ident.to_string());
        syn::visit::visit_item_type(self, i);
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        self.traits.insert(i.ident.to_string());
        syn::visit::visit_item_trait(self, i);
    }
}

/// A sorted set → a comma-joined string, or `None` when empty (so an empty fact
/// emits no node attribute).
pub(super) fn joined(set: &std::collections::BTreeSet<String>) -> Option<String> {
    if set.is_empty() {
        None
    } else {
        Some(set.iter().cloned().collect::<Vec<_>>().join(","))
    }
}

pub(super) fn convert_visibility(v: &SynVis) -> Visibility {
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

/// Read the string value of a `#[path = "..."]` attribute on a module, if present.
pub(super) fn mod_path_attr(m: &ItemMod) -> Option<String> {
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

pub(super) fn resolve_submodule_path(parent_file: &Path, mod_name: &str) -> Option<PathBuf> {
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
