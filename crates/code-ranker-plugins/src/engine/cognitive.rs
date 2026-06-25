//! Cognitive complexity: the shared rca state machine.
//!
//! A preorder DFS carrying `(nesting, depth, lambda)` top-down ([`CogCtx`]) and
//! accumulating `structural`, with `boolean_op` reset at branches and
//! saved/restored across spaces. The per-node increments (which kinds increase
//! nesting, the `+1` extras, boolean-run evaluation) are the genuinely-divergent
//! part and live in each [`Dialect`]'s `cog_node`; the boolean save/restore on a
//! space boundary is shared here.

use super::core::{CogCtx, CogState, Dialect};
use tree_sitter::Node;

pub fn walk<D: Dialect>(node: Node, ctx: CogCtx, d: &D, st: &mut CogState) {
    // The dialect applies its per-node increments and returns the child context.
    let child_ctx = d.cog_node(node, ctx, st);

    let r = d.roles();
    let id = node.kind_id();
    let is_space = r.space_kinds.contains(&id) || r.closure_space_kinds.contains(&id);

    let mut cursor = node.walk();
    if is_space {
        let saved = st.boolean_op;
        st.boolean_op = None;
        for child in node.children(&mut cursor) {
            walk(child, child_ctx, d, st);
        }
        st.boolean_op = saved;
    } else {
        for child in node.children(&mut cursor) {
            walk(child, child_ctx, d, st);
        }
    }
}
