//! The Rust plugin's merged config, in a leaf module so both `mod.rs` (which
//! builds `levels()` / `thresholds()` / `presets()` from it) and `collapse.rs`
//! (which reads the edge-kind identifiers from it) can depend on it *down* —
//! referencing `super::CONFIG` from `collapse.rs` would otherwise close a
//! `mod.rs ↔ collapse.rs` cycle (mod.rs already owns `collapse`). Same rationale
//! as `ids.rs`.

use std::sync::LazyLock;

/// The Rust config: `config.toml` deep-merged over the shared `defaults.toml`.
/// Drives `levels()` (edge/node-kind vocab) / `thresholds()` / `presets()` /
/// `metric_specs()` so `mod.rs` stays thin — the language-specific data lives in
/// `config.toml`, not in code.
pub(crate) static CONFIG: LazyLock<toml::Table> =
    LazyLock::new(|| crate::config::load(include_str!("config.toml")));
