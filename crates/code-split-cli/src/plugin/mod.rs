use anyhow::{Result, bail};
use code_split_graph::{PluginGraphs, StageTime};
use code_split_plugin_api::LanguagePlugin;
use std::path::Path;

/// The single place that knows which language plugins exist. Add a language by
/// writing a `code-split-plugin-<lang>` crate and adding one line here — the
/// rest of the CLI works only against the `LanguagePlugin` trait and never
/// names a concrete language.
pub fn registry() -> Vec<Box<dyn LanguagePlugin>> {
    vec![
        Box::new(code_split_plugin_rust::RustPlugin),
        Box::new(code_split_plugin_python::PythonPlugin),
        Box::new(code_split_plugin_javascript::JavascriptPlugin),
    ]
}

/// Comma-separated canonical plugin names, for help/error messages.
pub fn names() -> String {
    registry()
        .iter()
        .map(|p| p.name())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Run the plugin whose name (or alias) matches `name`.
pub fn run(name: &str, workspace: &Path) -> Result<(PluginGraphs, Vec<StageTime>)> {
    let reg = registry();
    match reg.iter().find(|p| p.matches(name)) {
        Some(p) => p.run(workspace),
        None => bail!("unknown plugin {name:?}; built-in plugins are: {}", names()),
    }
}

/// Tool/toolchain versions the matching plugin wants recorded in the snapshot.
pub fn versions(name: &str, workspace: &Path) -> Vec<(String, String)> {
    registry()
        .iter()
        .find(|p| p.matches(name))
        .map(|p| p.versions(workspace))
        .unwrap_or_default()
}

/// Auto-detect the plugin from workspace-root marker files. Errors if no plugin
/// matches, or if more than one does (ambiguous).
pub fn detect(workspace: &Path) -> Result<String> {
    let reg = registry();
    let found: Vec<&str> = reg
        .iter()
        .filter(|p| p.detect(workspace))
        .map(|p| p.name())
        .collect();
    match found.as_slice() {
        [one] => Ok((*one).to_string()),
        [] => bail!(
            "could not auto-detect a plugin in {}: no project marker found — \
             pass --plugin {}",
            workspace.display(),
            names()
        ),
        _ => bail!(
            "ambiguous project in {}: markers for multiple plugins found ({}) — \
             pass --plugin to choose",
            workspace.display(),
            found.join(", ")
        ),
    }
}
