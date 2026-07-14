//! Python module-path mapping + import resolution.
//!
//! The file→dotted-module-path mapping, the workspace module index, and the
//! resolver that turns one parsed import record into target file paths. Split
//! out of [`super`] (the structure builder) as a cohesive, behavior-identical
//! submodule. It owns the module-path DATA ([`MODULE`] + [`file_to_module_path`],
//! which the index and resolver need) and reads it from the Python leaf config
//! (`super::super::cfg`-style `crate::config::load(include_str!("config.toml"))`)
//! — so it depends only on `crate::config` downward and never back up on `super`
//! (keeping the Python module graph acyclic).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Module-path DATA the file→module mapping keys on, resolved once from
/// `python/config.toml`. The mapping LOGIC stays in Rust; the names it keys on
/// are data. `package_init_file` is the implicit package-init stem
/// (`pkg/__init__.py` → module `pkg`); `dot_exts` are the source extensions with
/// a leading dot (`[".py"]`, derived from `extensions`) that `file_to_module_path`
/// strips. Loaded self-contained (depending only on `crate::config`) rather than
/// via the parent `mod.rs`'s `CONFIG`, keeping the Python module graph acyclic.
pub(super) struct ModuleLists {
    pub(super) package_init_file: String,
    pub(super) dot_exts: Vec<String>,
}

pub(super) static MODULE: LazyLock<ModuleLists> = LazyLock::new(|| {
    let cfg = crate::config::load(include_str!("config.toml"));
    let extensions = crate::config::string_list(&cfg, "extensions");
    let dot_exts = extensions.iter().map(|e| format!(".{e}")).collect();
    ModuleLists {
        package_init_file: cfg
            .get("package_init_file")
            .and_then(|v| v.as_str())
            .expect("python/config.toml `package_init_file`")
            .to_string(),
        dot_exts,
    }
});

/// `parser/shops/amazon/pdp.py` → `"parser.shops.amazon.pdp"`
/// `parser/shops/amazon/__init__.py` → `"parser.shops.amazon"`
pub(super) fn file_to_module_path(workspace: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(workspace).ok()?;
    let mut parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();

    let last = parts.last_mut()?;
    if *last == MODULE.package_init_file {
        parts.pop();
    } else {
        let stem = MODULE
            .dot_exts
            .iter()
            .find_map(|e| last.strip_suffix(e.as_str()))?;
        *last = stem.to_string();
    }

    if parts.is_empty() {
        return None;
    }
    Some(parts.join("."))
}

pub(super) fn build_module_index(
    workspace: &Path,
    py_files: &[PathBuf],
) -> HashMap<String, PathBuf> {
    py_files
        .iter()
        .filter_map(|p| file_to_module_path(workspace, p).map(|m| (m, p.clone())))
        .collect()
}

// ---------------------------------------------------------------------------
// Import resolution
// ---------------------------------------------------------------------------

/// Resolve one import record to a set of target file paths in this project.
pub(super) fn resolve_import(
    base: &str,
    names: &[String],
    current_mod: &str,
    index: &HashMap<String, PathBuf>,
) -> Vec<PathBuf> {
    let abs_base = absolute_base(base, current_mod);
    let mut results: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    let mut try_add = |mod_path: &str| {
        if let Some(p) = index.get(mod_path)
            && seen.insert(p.clone())
        {
            results.push(p.clone());
        }
    };

    if names.is_empty() {
        // plain `import X.Y.Z`
        try_add(&abs_base);
    } else {
        for name in names {
            let full = if abs_base.is_empty() {
                name.clone()
            } else {
                format!("{abs_base}.{name}")
            };
            try_add(&full);
        }
        // Also add the base itself (might import symbols from it).
        if !abs_base.is_empty() {
            try_add(&abs_base);
        }
    }

    results
}

/// Turn a possibly-relative base like `"."`, `".utils"`, `"..shops"` into
/// an absolute dotted module path using `current_mod` as the anchor.
pub(super) fn absolute_base(base: &str, current_mod: &str) -> String {
    if !base.starts_with('.') {
        return base.to_string();
    }

    let dots = base.chars().take_while(|&c| c == '.').count();
    let suffix = base[dots..].to_string(); // part after dots (may be empty)

    let parts: Vec<&str> = current_mod.split('.').collect();
    let keep = parts.len().saturating_sub(dots);
    let pkg = parts[..keep].join(".");

    if suffix.is_empty() {
        pkg
    } else if pkg.is_empty() {
        suffix
    } else {
        format!("{pkg}.{suffix}")
    }
}
