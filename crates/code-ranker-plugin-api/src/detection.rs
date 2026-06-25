//! Generic, language-agnostic project-detection helpers shared by plugins.
//!
//! These answer "is this a project of kind X on disk?" — a concern distinct from
//! the parsing contract in [`plugin`](crate::plugin). Keeping them here lets every
//! plugin reuse the helper without depending on a sibling plugin or pulling
//! detection logic into the contract.

use std::path::Path;

/// Return `true` when `workspace` contains the given marker file. A generic,
/// language-agnostic detection helper for marker-based plugins (e.g. JS →
/// `"package.json"`, TS → `"tsconfig.json"`).
pub fn detect_with_marker(workspace: &Path, marker: &str) -> bool {
    workspace.join(marker).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_with_marker_checks_file_presence() {
        let dir = std::env::temp_dir();
        // a marker that (almost certainly) does not exist
        assert!(!detect_with_marker(&dir, "code-ranker-no-such-marker.xyz"));
    }
}
