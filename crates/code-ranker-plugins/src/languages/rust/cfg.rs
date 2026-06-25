//! The Rust plugin's merged config, in a leaf module so both `mod.rs` (which
//! builds `levels()` / `thresholds()` / `principles()` from it) and `collapse.rs`
//! (which reads the edge-kind identifiers from it) can depend on it *down* —
//! referencing `super::CONFIG` from `collapse.rs` would otherwise close a
//! `mod.rs ↔ collapse.rs` cycle (mod.rs already owns `collapse`). Same rationale
//! as `ids.rs`.

use std::collections::BTreeMap;
use std::sync::LazyLock;

/// The Rust config: `config.toml` deep-merged over the shared `defaults.toml`.
/// Drives `levels()` (edge/node-kind vocab) / `thresholds()` / `principles()` /
/// `metric_specs()` so `mod.rs` stays thin — the language-specific data lives in
/// `config.toml`, not in code.
pub(crate) static CONFIG: LazyLock<toml::Table> =
    LazyLock::new(|| crate::config::load(include_str!("config.toml")));

/// One `[ids]` namespace prefix (`crate:` / `mod:` / `ext:`), resolved from the
/// merged config. Panics if the key is absent — it is a build-time authoring bug.
fn id_prefix(key: &str) -> String {
    crate::config::string_table(&CONFIG, "ids")
        .get(key)
        .cloned()
        .unwrap_or_else(|| panic!("rust/config.toml [ids] is missing `{key}`"))
}

/// One top-level scalar string key, resolved from the merged config (panics if
/// absent — a build-time authoring bug).
fn scalar(key: &str) -> String {
    CONFIG
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("rust/config.toml `{key}` is missing"))
        .to_string()
}

/// `crate:` node-id prefix (a crate node). From `[ids]`.
pub(crate) static ID_CRATE: LazyLock<String> = LazyLock::new(|| id_prefix("crate"));
/// `mod:` node-id prefix (a module node). From `[ids]`.
pub(crate) static ID_MODULE: LazyLock<String> = LazyLock::new(|| id_prefix("module"));
/// `ext:` node-id prefix (an external node), inherited from `defaults.toml`.
pub(crate) static ID_EXTERNAL: LazyLock<String> = LazyLock::new(|| id_prefix("external"));

/// Cargo target kinds addressable by name from another crate (`is_lib_target`).
pub(crate) static IMPORTABLE_TARGETS: LazyLock<Vec<String>> =
    LazyLock::new(|| crate::config::string_list(&CONFIG, "importable_targets"));
/// Cargo target kinds whose module tree is walked (`is_supported_target`).
pub(crate) static SUPPORTED_TARGETS: LazyLock<Vec<String>> =
    LazyLock::new(|| crate::config::string_list(&CONFIG, "supported_targets"));
/// Standard-distribution crate roots that resolve to no workspace edge.
pub(crate) static STD_CRATES: LazyLock<Vec<String>> =
    LazyLock::new(|| crate::config::string_list(&CONFIG, "std_crates"));
/// File stems whose submodules live in the same directory (`lib`/`main`/`mod`).
pub(crate) static MODULE_ROOTS: LazyLock<Vec<String>> =
    LazyLock::new(|| crate::config::string_list(&CONFIG, "module_roots"));
/// The Rust source-file extension a `mod <name>;` resolves to (no leading dot).
pub(crate) static SOURCE_EXT: LazyLock<String> = LazyLock::new(|| scalar("source_ext"));
/// The directory-module file name (`<dir>/<name>/mod.rs`).
pub(crate) static DIR_MODULE_FILE: LazyLock<String> = LazyLock::new(|| scalar("dir_module_file"));

/// Visibility output strings from `[visibility]`, by classification slot.
static VISIBILITY: LazyLock<BTreeMap<String, String>> =
    LazyLock::new(|| crate::config::string_table(&CONFIG, "visibility"));

/// The output string for a visibility classification slot (`public` / `crate` /
/// `super` / `private`), from `[visibility]`. Panics on a missing slot.
pub(crate) fn visibility(slot: &str) -> &'static str {
    VISIBILITY
        .get(slot)
        .map(String::as_str)
        .unwrap_or_else(|| panic!("rust/config.toml [visibility] is missing `{slot}`"))
}

/// A `[fields]` / `[syn]` / `[path_keywords]` entry, resolved from the named
/// table by slot. Panics on a missing slot (a build-time authoring bug).
fn vocab(section: &str, slot: &str) -> String {
    crate::config::string_table(&CONFIG, section)
        .get(slot)
        .cloned()
        .unwrap_or_else(|| panic!("rust/config.toml [{section}] is missing `{slot}`"))
}

/// tree-sitter `return_type` field name (`[fields]`).
pub(crate) static FIELD_RETURN_TYPE: LazyLock<String> =
    LazyLock::new(|| vocab("fields", "return_type"));

/// `syn` attribute idents the test/derive/path walk matches (`[syn]`).
pub(crate) static SYN_TEST: LazyLock<String> = LazyLock::new(|| vocab("syn", "test"));
pub(crate) static SYN_BENCH: LazyLock<String> = LazyLock::new(|| vocab("syn", "bench"));
pub(crate) static SYN_CFG: LazyLock<String> = LazyLock::new(|| vocab("syn", "cfg"));
pub(crate) static SYN_DERIVE: LazyLock<String> = LazyLock::new(|| vocab("syn", "derive"));
pub(crate) static SYN_PATH: LazyLock<String> = LazyLock::new(|| vocab("syn", "path"));

/// Rust path roots the `use` resolver dispatches on (`[path_keywords]`).
pub(crate) static PK_CRATE: LazyLock<String> = LazyLock::new(|| vocab("path_keywords", "crate"));
pub(crate) static PK_SELF: LazyLock<String> = LazyLock::new(|| vocab("path_keywords", "self"));
pub(crate) static PK_SUPER: LazyLock<String> = LazyLock::new(|| vocab("path_keywords", "super"));
