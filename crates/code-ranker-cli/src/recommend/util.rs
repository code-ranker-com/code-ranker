//! Small dependency-free formatting/label helpers shared across the recommend
//! submodules. Kept in a leaf module (no `recommend` back-edge) so importing them
//! does not couple a consumer to the recommend hub.

use code_ranker_graph::level_graph::LevelGraph;

/// Strip a leading `{root}/` token from a relativized id, e.g.
/// `{target}/src/a.rs` → `src/a.rs`. A file node's id IS its path.
pub(crate) fn clean_path(id: &str) -> String {
    if let Some(rest) = id.strip_prefix('{')
        && let Some(idx) = rest.find("}/")
    {
        return rest[idx + 2..].to_string();
    }
    id.to_string()
}

/// The short header label for a metric (falls back to its label, then the key).
pub(crate) fn attr_short<'a>(level: &'a LevelGraph, metric: &'a str) -> &'a str {
    level
        .node_attributes
        .get(metric)
        .and_then(|s| s.short.as_deref().or(s.label.as_deref()))
        .unwrap_or(metric)
}

/// Format a metric value for CLI output: the exact rounded integer (never
/// abbreviated — the K/M/G `abbreviate` spec flag is a viewer-only concern, so the
/// scorecard and prompt always show the precise number, e.g. `295488` not `295.5K`).
pub(crate) fn fmt_val(v: f64) -> String {
    format!("{}", v.round() as i64)
}
