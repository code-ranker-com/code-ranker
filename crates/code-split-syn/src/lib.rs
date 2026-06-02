mod crate_graph;
mod ids;
mod module_graph;

use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use code_split_core::GraphBuilder;
use std::path::Path;

pub fn analyze(workspace: &Path, builder: &mut GraphBuilder) -> Result<()> {
    let manifest = workspace.join("Cargo.toml");
    let metadata = MetadataCommand::new()
        .manifest_path(&manifest)
        .exec()
        .with_context(|| format!("running cargo metadata for {}", manifest.display()))?;

    crate_graph::contribute(&metadata, builder);
    module_graph::contribute(&metadata, builder)?;
    Ok(())
}
