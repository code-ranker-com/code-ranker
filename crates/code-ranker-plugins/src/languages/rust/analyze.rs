//! Syntactic analysis driver: resolve the workspace via `cargo metadata` and
//! build the internal crate + module/use graphs (`crate_graph` + `module_graph`).

use anyhow::Result;
use cargo_metadata::MetadataCommand;
use code_ranker_plugin_api::log;
use std::path::Path;

use super::crate_graph;
use super::internal::GraphBuilder;
use super::module_graph;

/// Syntactic stage: resolve the workspace via `cargo metadata` and build the
/// internal crate + module/use graphs.
pub(super) fn syn_analyze(
    workspace: &Path,
    ignore_tests: bool,
    builder: &mut GraphBuilder,
) -> Result<()> {
    let manifest = workspace.join("Cargo.toml");
    // code-ranker is an offline tool: it never fetches from the network. See the
    // comment in the original lib.rs for the research notes on --offline vs
    // --no-deps vs full. Short version: --offline keeps external/cross-crate
    // edges AND never goes to the network; the cache must be warm.
    let metadata = log::timed("cargo metadata --offline", || {
        MetadataCommand::new()
            .manifest_path(&manifest)
            .other_options(vec!["--offline".to_string()])
            .exec()
    })
    .map_err(|err| offline_metadata_error(&manifest, err))?;

    crate_graph::contribute(&metadata, builder);
    module_graph::contribute(&metadata, ignore_tests, builder)?;
    Ok(())
}

pub(super) fn offline_metadata_error(manifest: &Path, err: cargo_metadata::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "cargo metadata (offline) failed for {manifest}\n\n\
         code-ranker is an offline tool — it never downloads dependencies. It reads \
         the dependency graph from cargo's local cache, which must already be \
         populated for this project.\n\n\
         Warm the cache once (with network), then re-run code-ranker:\n    \
         cargo metadata --manifest-path {manifest} >/dev/null\n\
         (a prior `cargo build` / `cargo fetch` works too).\n\n\
         In CI: run code-ranker on the same image/cache as your build or test jobs, \
         where the cache is already warm.\n\n\
         Underlying cargo error: {err}",
        manifest = manifest.display(),
    )
}
