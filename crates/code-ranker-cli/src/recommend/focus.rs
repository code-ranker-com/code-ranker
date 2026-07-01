//! `--focus-path` resolution and matching for the scorecard / `--prompt` ranking
//! and the `check` gate. Kept in one place so both paths scope identically.

use super::util::clean_path;
use code_ranker_plugin_api::node::Node;

/// A `--focus-path` set resolved against the analysis target, so the same subtree
/// matches however the path was written: the `[input]` path itself, that path
/// plus a subfolder, an absolute path, or a target-relative subpath (`src`). The
/// resolution runs once (not per node); the target is kept for the absolute-path
/// reconstruction in [`FocusPaths::matches_id`].
///
/// File nodes under `[input]` are relativized to `{target}/rel`; other crates keep
/// a `{root}/rel` token or an absolute path. So an entry that names the target
/// subtree matches only `{target}` nodes, while a bare subpath (`src`) or an
/// ancestor/absolute path matches the printed / reconstructed location.
#[derive(Debug, Clone)]
pub(crate) struct FocusPaths {
    entries: Vec<FocusEntry>,
    /// Absolute analysis target (`snapshot.target`) — reconstructs a target node's
    /// absolute path so an ancestor/absolute `--focus-path` can match it.
    target: String,
}

#[derive(Debug, Clone)]
struct FocusEntry {
    /// `Some(sub)` when the raw entry names the target subtree (`sub` is the path
    /// under it, `""` = the whole target). `None` = match the printed location
    /// directly (a target-relative subpath such as `src`, or an ancestor/absolute
    /// path matched against the reconstructed location).
    under_target: Option<String>,
    /// The trimmed raw entry, used for the `None` (printed/absolute) match.
    raw: String,
}

impl FocusPaths {
    pub(crate) fn new(raw: &[String], target: &str) -> Self {
        let entries = raw
            .iter()
            .filter_map(|f| {
                let f = f.trim_start_matches("./").trim_end_matches('/');
                (!f.is_empty()).then(|| FocusEntry {
                    under_target: target_subpath(f, target),
                    raw: f.to_string(),
                })
            })
            .collect();
        Self {
            entries,
            target: target.trim_end_matches('/').to_string(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Match a raw (token-carrying) node id — the recommend / scorecard path.
    pub(crate) fn matches_id(&self, id: &str) -> bool {
        let under = id.strip_prefix("{target}/");
        let rel = clean_path(id);
        let abs = under.map(|r| format!("{}/{r}", self.target));
        self.entries
            .iter()
            .any(|e| e.matches(under, &rel, abs.as_deref()))
    }

    /// Match a location already stripped to a target-relative path — the `check`
    /// gate path (`violation_rel_path` yields `Some(rel)` only for `{target}`
    /// violations, so the node is known to sit under the target).
    pub(crate) fn matches_target_rel(&self, rel: &str) -> bool {
        let abs = format!("{}/{rel}", self.target);
        self.entries
            .iter()
            .any(|e| e.matches(Some(rel), rel, Some(&abs)))
    }
}

impl FocusEntry {
    fn matches(&self, under: Option<&str>, rel: &str, abs: Option<&str>) -> bool {
        match &self.under_target {
            // The entry names the target subtree → only nodes under `{target}`.
            Some(sub) => under.is_some_and(|r| under_matches(sub, r)),
            // Otherwise match the printed location, or the reconstructed absolute
            // path (so an ancestor/absolute `--focus-path` still matches a target
            // node whose printed form dropped the absolute prefix).
            None => {
                prefix_matches(&self.raw, rel) || abs.is_some_and(|a| prefix_matches(&self.raw, a))
            }
        }
    }
}

/// Whether `rel` is at or under the target-relative `sub` (`""` = the whole target).
fn under_matches(sub: &str, rel: &str) -> bool {
    sub.is_empty() || rel == sub || rel.starts_with(&format!("{sub}/"))
}

/// Whether `path` equals `prefix` or is a file/folder beneath it.
fn prefix_matches(prefix: &str, path: &str) -> bool {
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

/// If `entry` names the analysis `target` or a path under it — written as the same
/// (relative) `[input]` path or as an absolute path — return the remainder under
/// the target (`""` when it *is* the target). `None` for a bare subpath (`src`) or
/// an unrelated/ancestor path. Resolved once per entry in [`FocusPaths::new`].
fn target_subpath(entry: &str, target: &str) -> Option<String> {
    let t = target.trim_start_matches("./").trim_end_matches('/');
    // Direct string relation: the user passed the same (relative) path as `[input]`.
    if !t.is_empty() && t != "." {
        if entry == t {
            return Some(String::new());
        }
        if let Some(rest) = entry.strip_prefix(&format!("{t}/")) {
            return Some(rest.to_string());
        }
    }
    // Absolute relation: resolve both against the CWD lexically (no filesystem
    // canonicalize, so reading a snapshot on another machine never panics).
    let at = abs_lexical(t);
    let af = abs_lexical(entry);
    if af == at {
        return Some(String::new());
    }
    af.strip_prefix(&format!("{at}/")).map(str::to_string)
}

/// Lexically absolutize a path against the CWD (join if relative; leave an
/// absolute path untouched). No symlink/`..` resolution and no filesystem access —
/// a best-effort string form for prefix comparison.
fn abs_lexical(path: &str) -> String {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        return path.trim_end_matches('/').to_string();
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd
            .join(p)
            .to_string_lossy()
            .trim_end_matches('/')
            .to_string(),
        Err(_) => path.trim_end_matches('/').to_string(),
    }
}

/// Whether `node` falls under one of the `--focus-path` entries (empty = no
/// restriction). See [`FocusPaths`] for how an entry is matched.
pub(crate) fn in_focus(node: &Node, focus: &FocusPaths) -> bool {
    focus.is_empty() || focus.matches_id(&node.id)
}
