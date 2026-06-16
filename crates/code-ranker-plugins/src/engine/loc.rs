//! LOC breakdown — a faithful port of rca's `Loc::compute`: a preorder over ALL
//! nodes (named + anonymous). Code-bearing tokens insert their start row into
//! `lines` (→ ploc); comments accumulate cloc with rca's same/independent-line
//! logic; statement nodes count lloc; blank = (root span) − ploc − only_comment.
//!
//! The shared default classifies by the `noop_kinds` / `comment_kinds` /
//! `statement_kinds` role sets. A dialect's `loc_node` runs first and may fully
//! handle a node (rust's `line_comment` with a `doc_comment` child; python's
//! `string` docstring/code distinction), returning `true` to skip the default.

use super::core::{Dialect, LocState};
use tree_sitter::Node;

pub fn compute<D: Dialect>(root: Node, d: &D) -> LocState {
    let mut st = LocState::default();
    walk(root, d, &mut st);
    st.ploc = st.lines.len();
    // sloc span of the unit (source_file): rca uses end - start for the unit.
    let span = root
        .end_position()
        .row
        .saturating_sub(root.start_position().row) as i64;
    st.blank = span - st.ploc as i64 - st.only_comment;
    st
}

fn walk<D: Dialect>(node: Node, d: &D, st: &mut LocState) {
    if !d.loc_node(node, st) {
        let r = d.roles();
        let id = node.kind_id();
        let start = node.start_position().row;
        let end = node.end_position().row;
        if r.noop_kinds.contains(&id) {
            // no LOC contribution
        } else if r.comment_kinds.contains(&id) {
            add_cloc_lines(st, start, end);
        } else if r.statement_kinds.contains(&id) {
            st.lloc += 1;
        } else {
            check_comment_ends_on_code_line(st, start);
            st.lines.insert(start);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, d, st);
    }
}

pub fn add_cloc_lines(st: &mut LocState, start: usize, end: usize) {
    let comment_diff = end - start;
    let after_code = st.lines.contains(&start);
    if after_code && comment_diff == 0 {
        st.code_comment += 1;
    } else if after_code && comment_diff > 0 {
        st.code_comment += 1;
        st.only_comment += comment_diff as i64;
    } else {
        st.only_comment += (comment_diff + 1) as i64;
        st.comment_line_end = Some(end);
    }
}

pub fn check_comment_ends_on_code_line(st: &mut LocState, start_code_line: usize) {
    if let Some(end) = st.comment_line_end
        && end == start_code_line
        && !st.lines.contains(&start_code_line)
    {
        st.only_comment -= 1;
        st.code_comment += 1;
    }
}

/// Whether `node` has a direct child with kind id `kind` (used by rust's
/// `line_comment` + `doc_comment` LOC special-case).
pub fn has_child_kind(node: Node, kind: u16) -> bool {
    if kind == u16::MAX {
        return false;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|c| c.kind_id() == kind)
}
