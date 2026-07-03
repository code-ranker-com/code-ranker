//! Graph filtering before cycles/metrics: ignore globs and dev-only crates.
//! Owns the external-node predicate. Test files are dropped earlier, by the
//! language plugin during its walk (see `PluginInput::ignore_tests`).

use super::model::IgnoreConfig;
use anyhow::{Context, Result};
use code_ranker_plugin_api::{attrs::AttrValue, graph::Graph, log, node::Node};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

pub(crate) fn is_external(node: &Node) -> bool {
    node.kind == "external" || matches!(node.attrs.get("external"), Some(AttrValue::Bool(true)))
}

/// Strip nodes/edges matching ignore globs or dev-only crates from the
/// structural graph (before cycles/metrics). Test files are already gone —
/// dropped by the language plugin during its walk.
pub fn apply_ignore(graph: &mut Graph, ignore: &IgnoreConfig, target: &Path) -> Result<usize> {
    // Test files are dropped earlier — by the language plugin during its walk,
    // which knows the language's test conventions (see `PluginInput::ignore_tests`).
    // Here we only apply the language-agnostic
    // glob filter and dev-only-crate pruning of external nodes.
    let gs = if ignore.paths.is_empty() {
        None
    } else {
        Some(build_glob_set(&ignore.paths)?)
    };
    let dev_only = if ignore.dev_only_crates {
        collect_dev_only_crates(target)
    } else {
        HashSet::new()
    };
    if gs.is_none() && dev_only.is_empty() {
        return Ok(0);
    }
    Ok(filter_graph(graph, gs.as_ref(), &dev_only))
}

fn collect_dev_only_crates(target: &Path) -> HashSet<String> {
    let out = log::timed("cargo metadata (dev-only crates)", || {
        std::process::Command::new("cargo")
            .args(["metadata", "--format-version", "1"])
            .current_dir(target)
            .stderr(std::process::Stdio::null())
            .output()
            .expect("cargo metadata failed — is cargo installed?")
    });
    assert!(
        out.status.success(),
        "cargo metadata exited with {}",
        out.status
    );

    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("cargo metadata produced invalid JSON");
    dev_only_crates_from_metadata(&meta)
}

/// The pure half of [`collect_dev_only_crates`]: BFS the resolved dependency
/// graph from the workspace members over non-dev edges only, so any package
/// never reached that way is dev-only. Split out so the graph logic is
/// testable against a hand-built `cargo metadata` JSON value, without
/// shelling out to `cargo`.
fn dev_only_crates_from_metadata(meta: &serde_json::Value) -> HashSet<String> {
    let packages = meta["packages"].as_array().expect("packages array");
    let mut id_to_name: HashMap<&str, &str> = HashMap::new();
    for pkg in packages {
        if let (Some(id), Some(name)) = (pkg["id"].as_str(), pkg["name"].as_str()) {
            id_to_name.insert(id, name);
        }
    }

    let workspace_members: HashSet<&str> = meta["workspace_members"]
        .as_array()
        .expect("workspace_members array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();

    let nodes = meta["resolve"]["nodes"]
        .as_array()
        .expect("resolve.nodes array");
    let mut adj: HashMap<&str, Vec<(&str, bool)>> = HashMap::new();
    for node in nodes {
        let Some(id) = node["id"].as_str() else {
            continue;
        };
        let Some(deps) = node["deps"].as_array() else {
            continue;
        };
        let edges = deps
            .iter()
            .filter_map(|dep| {
                let dep_id = dep["pkg"].as_str()?;
                let kinds = dep["dep_kinds"].as_array()?;
                let dev_only = kinds.iter().all(|k| k["kind"].as_str() == Some("dev"));
                Some((dep_id, dev_only))
            })
            .collect();
        adj.insert(id, edges);
    }

    let mut regular: HashSet<&str> = workspace_members.iter().copied().collect();
    let mut queue: VecDeque<&str> = regular.iter().copied().collect();
    while let Some(id) = queue.pop_front() {
        for &(dep_id, dev_only) in adj.get(id).map(Vec::as_slice).unwrap_or(&[]) {
            if !dev_only && regular.insert(dep_id) {
                queue.push_back(dep_id);
            }
        }
    }

    adj.keys()
        .filter(|&&id| !regular.contains(id))
        .filter_map(|&id| id_to_name.get(id).map(|n| n.to_string()))
        .collect()
}

fn build_glob_set(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).with_context(|| format!("invalid glob: {p}"))?);
    }
    Ok(b.build()?)
}

/// Ids are `{root}/sub/path` after relativize; strip the `{…}/` prefix for glob
/// matching. External ids (`ext:name`) are returned as-is.
fn strip_root_prefix(id: &str) -> &str {
    if id.starts_with('{')
        && let Some(idx) = id.find('}')
    {
        return id[idx + 1..].trim_start_matches('/');
    }
    id
}

fn filter_graph(graph: &mut Graph, gs: Option<&GlobSet>, dev_only: &HashSet<String>) -> usize {
    let removed: HashSet<String> = graph
        .nodes
        .iter()
        .filter(|n| {
            if is_external(n) {
                if !dev_only.is_empty()
                    && let Some(name) = n.id.strip_prefix("ext:")
                {
                    let base = name.split('@').next().unwrap_or(name);
                    return dev_only.contains(base);
                }
                return false;
            }
            if let Some(gs) = gs
                && gs.is_match(strip_root_prefix(&n.id))
            {
                return true;
            }
            false
        })
        .map(|n| n.id.clone())
        .collect();
    if removed.is_empty() {
        return 0;
    }
    let before = graph.nodes.len();
    graph.nodes.retain(|n| !removed.contains(&n.id));
    graph
        .edges
        .retain(|e| !removed.contains(&e.source) && !removed.contains(&e.target));
    before - graph.nodes.len()
}

#[cfg(test)]
#[path = "ignore_test.rs"]
mod tests;
