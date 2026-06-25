//! The shared ECMAScript merged config, as a LEAF module.
//!
//! Both `mod.rs` (level descriptors: `[node_kinds.arrow]`/`[node_kinds.generator]`,
//! the import-graph `[structure]` vocab) and `dialect.rs` (the function-unit
//! `[units]` id strings) read the same merged `ecmascript/config.toml`. Keeping
//! the static here — depended on by both, depending on neither — keeps the
//! ECMAScript module graph acyclic (avoids the `code-ranker check .` self-check
//! cycle gate that a sibling-to-sibling `CONFIG` read would trip).

use std::sync::LazyLock;

/// `ecmascript/config.toml` deep-merged over the shared `defaults.toml`.
pub static CONFIG: LazyLock<toml::Table> =
    LazyLock::new(|| crate::config::load(include_str!("config.toml")));

/// The `ext:` node-id namespace prefix for an external (third-party) node,
/// inherited from `defaults.toml`'s `[ids].external`. Resolved once here so both
/// the JS and TS plugins build external ids from the shared data, not a literal.
pub static EXTERNAL_ID_PREFIX: LazyLock<String> = LazyLock::new(|| {
    crate::config::string_table(&CONFIG, "ids")
        .get("external")
        .cloned()
        .expect("ecmascript [ids].external (inherited from defaults.toml)")
});

/// The `public` visibility output string, inherited from `defaults.toml`'s
/// `[visibility].public` (every ECMAScript file node is public).
pub static VISIBILITY_PUBLIC: LazyLock<String> = LazyLock::new(|| {
    crate::config::string_table(&CONFIG, "visibility")
        .get("public")
        .cloned()
        .expect("ecmascript [visibility].public (inherited from defaults.toml)")
});
