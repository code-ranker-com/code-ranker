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
