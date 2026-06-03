//! The language-plugin interface. Everything outside the `code-split-plugin-*`
//! crates works only against this trait — it never names a concrete language.
//! Adding a language is: write a crate that implements `LanguagePlugin`, then
//! register one instance in the CLI's plugin list. Nothing else changes.

use anyhow::Result;
use code_split_graph::{PluginGraphs, StageTime};
use std::path::Path;

pub trait LanguagePlugin {
    /// Canonical name, e.g. `"rust"`. Used by `--plugin` and in the snapshot.
    fn name(&self) -> &'static str;

    /// Extra names accepted on the CLI, e.g. `["typescript", "js", "ts"]`.
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    /// Workspace-root marker files that indicate this language, e.g.
    /// `["Cargo.toml"]`. Used for `--plugin auto` detection.
    fn markers(&self) -> &'static [&'static str];

    /// Analyze the workspace and produce the file graph plus stage timings.
    fn run(&self, workspace: &Path) -> Result<(PluginGraphs, Vec<StageTime>)>;

    /// Tool/toolchain versions to record in the snapshot (e.g. `("rustc", "1.88")`).
    /// Default: none.
    fn versions(&self, _workspace: &Path) -> Vec<(String, String)> {
        Vec::new()
    }

    /// True if `query` matches this plugin's name or any alias.
    fn matches(&self, query: &str) -> bool {
        self.name() == query || self.aliases().contains(&query)
    }

    /// True if any of this plugin's marker files exists at the workspace root.
    fn detect(&self, workspace: &Path) -> bool {
        self.markers().iter().any(|m| workspace.join(m).exists())
    }
}
