pub mod finalize;
pub mod javascript;
pub mod python;
pub mod rust;

use anyhow::{Result, bail};
use code_split_core::{PluginGraphs, StageTime};
use std::path::Path;

/// Run a built-in plugin for the given workspace. Returns `(graphs, timings)`.
///
/// All plugins are compiled into the binary and run in-process — there is no
/// external/dynamic plugin loading.
pub fn run(name: &str, workspace: &Path) -> Result<(PluginGraphs, Vec<StageTime>)> {
    match name {
        "rust" => rust::run(workspace),
        "python" => python::run(workspace),
        "javascript" | "typescript" | "js" | "ts" => javascript::run(workspace),
        other => bail!("unknown plugin {other:?}; built-in plugins are: rust, python, javascript"),
    }
}
