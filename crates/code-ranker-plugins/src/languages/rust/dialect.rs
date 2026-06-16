//! Rust [`Dialect`] for the generic metric engine.
//!
//! The walk logic lives in `crate::engine`; this is the thin Rust-specific layer:
//! the grammar, the resolved [`Roles`] (from `rust.toml`), and the few predicates
//! that genuinely differ for Rust — the Halstead operator context exceptions
//! (`||`/`/` only in a `binary_expression`; `!` unless in an
//! `inner_doc_comment_marker`), the `-> T` return-type extra exit, the cognitive
//! state machine (else-clause +1, labeled break/continue, function nesting), and
//! the `line_comment` + `doc_comment` LOC adjustment.

use crate::engine::{
    self, CogCtx, CogState, Dialect, HalClass, LocState, RoleCfg, Roles, UnitKind,
};
use code_ranker_plugin_api::metrics::{FunctionUnit, MetricInputs};
use std::sync::LazyLock;
use tree_sitter::{Language, Node};

static ROLE_CFG: LazyLock<RoleCfg> = LazyLock::new(|| {
    super::cfg::CONFIG
        .clone()
        .try_into()
        .expect("rust.toml [roles]/[halstead]/[loc] parse")
});

struct RustDialect {
    lang: Language,
    roles: Roles,
    // function-unit `kind` id strings from `[units]` (the classification logic is
    // below; only the emitted ids are data).
    unit_method: String,
    unit_default: String,
    // singleton ids cached for the divergent predicates
    function_item: u16,
    closure_expression: u16,
    impl_item: u16,
    trait_item: u16,
    else_clause: u16,
    binary_expression: u16,
    unary_expression: u16,
    break_expression: u16,
    continue_expression: u16,
    label: u16,
    if_expression: u16,
    amp_amp: u16,
    pipe_pipe: u16,
    // halstead special
    hal_pipe_pipe: u16,
    hal_slash: u16,
    hal_bang: u16,
    hal_binary_expression: u16,
    hal_inner_doc_comment_marker: u16,
    // loc special
    loc_line_comment: u16,
    loc_doc_comment: u16,
}

impl RustDialect {
    fn new() -> Self {
        let lang: Language = tree_sitter_rust::LANGUAGE.into();
        let roles = Roles::resolve(&lang, &ROLE_CFG);
        let one = |k: &str| roles.one(k);
        let sp = |k: &str| roles.special(k);
        let units = crate::config::units(&super::cfg::CONFIG);
        let unit = |k: &str| units.get(k).cloned().expect("[units] key");
        RustDialect {
            unit_method: unit("method"),
            unit_default: unit("default"),
            function_item: one("function_item"),
            closure_expression: one("closure_expression"),
            impl_item: one("impl_item"),
            trait_item: one("trait_item"),
            else_clause: one("else_clause"),
            binary_expression: one("binary_expression"),
            unary_expression: one("unary_expression"),
            break_expression: one("break_expression"),
            continue_expression: one("continue_expression"),
            label: one("label"),
            if_expression: one("if_expression"),
            amp_amp: one("amp_amp"),
            pipe_pipe: one("pipe_pipe"),
            hal_pipe_pipe: sp("pipe_pipe"),
            hal_slash: sp("slash"),
            hal_bang: sp("bang"),
            hal_binary_expression: sp("binary_expression"),
            hal_inner_doc_comment_marker: sp("inner_doc_comment_marker"),
            loc_line_comment: sp("line_comment"),
            loc_doc_comment: sp("doc_comment"),
            lang,
            roles,
        }
    }

    fn is_else_if(&self, node: Node) -> bool {
        node.parent()
            .is_some_and(|p| p.kind_id() == self.else_clause)
    }
}

static DIALECT: LazyLock<RustDialect> = LazyLock::new(RustDialect::new);

impl Dialect for RustDialect {
    fn language(&self) -> &Language {
        &self.lang
    }
    fn roles(&self) -> &Roles {
        &self.roles
    }

    fn file_initial_spaces(&self) -> u32 {
        1 // the source_file (unit) space
    }

    fn classify_unit(&self, node: Node) -> Option<UnitKind> {
        let id = node.kind_id();
        if id == self.function_item {
            Some(UnitKind::Func)
        } else if id == self.closure_expression {
            Some(UnitKind::Closure)
        } else {
            None
        }
    }

    fn extra_exits(&self, node: Node) -> u32 {
        // A value-returning exit when the fn declares a return type (`-> T`). The
        // tree-sitter field name is DATA (`[fields].return_type`).
        if node.kind_id() == self.function_item
            && node
                .child_by_field_name(super::cfg::FIELD_RETURN_TYPE.as_str())
                .is_some()
        {
            1
        } else {
            0
        }
    }

    fn cog_node(&self, node: Node, ctx: CogCtx, st: &mut CogState) -> CogCtx {
        let id = node.kind_id();
        let CogCtx {
            nesting,
            depth,
            lambda,
        } = ctx;
        let (mut cn, mut cd, mut cl) = (nesting, depth, lambda);

        if id == self.if_expression {
            if !self.is_else_if(node) {
                st.structural += nesting + depth + lambda + 1;
                cn = nesting + 1;
                st.boolean_op = None;
            }
        } else if self.roles.cog_nest.contains(&id) {
            // for_expression / while_expression / match_expression
            st.structural += nesting + depth + lambda + 1;
            cn = nesting + 1;
            st.boolean_op = None;
        } else if id == self.else_clause {
            st.structural += 1; // covers plain `else` and `else if`
        } else if id == self.break_expression || id == self.continue_expression {
            if let Some(lbl) = node.child(1)
                && lbl.kind_id() == self.label
            {
                st.structural += 1;
            }
        } else if id == self.unary_expression {
            st.boolean_op = Some(id); // not_operator
        } else if id == self.binary_expression {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                let cid = child.kind_id();
                if cid == self.amp_amp || cid == self.pipe_pipe {
                    st.eval_boolean(cid);
                }
            }
        } else if id == self.function_item {
            cn = 0;
            if engine::has_ancestor_id(node, self.function_item) {
                cd = depth + 1;
            }
        } else if id == self.closure_expression {
            cl = lambda + 1;
        }

        CogCtx {
            nesting: cn,
            depth: cd,
            lambda: cl,
        }
    }

    fn is_function_unit(&self, node: Node) -> bool {
        node.kind_id() == self.function_item
    }

    fn fn_kind(&self, node: Node) -> &str {
        // `method` when the nearest enclosing item is an `impl` / `trait`.
        let mut p = node.parent();
        while let Some(n) = p {
            if n.kind_id() == self.impl_item || n.kind_id() == self.trait_item {
                return &self.unit_method;
            }
            if n.kind_id() == self.function_item {
                return &self.unit_default;
            }
            p = n.parent();
        }
        &self.unit_default
    }

    fn hal_classify(&self, node: Node) -> HalClass {
        let id = node.kind_id();
        let is_operator = if id == self.hal_pipe_pipe || id == self.hal_slash {
            node.parent()
                .is_some_and(|p| p.kind_id() == self.hal_binary_expression)
        } else if id == self.hal_bang {
            node.parent()
                .is_none_or(|p| p.kind_id() != self.hal_inner_doc_comment_marker)
        } else {
            self.roles.operators.contains(&id)
        };
        if is_operator {
            HalClass::Operator
        } else if self.roles.operands.contains(&id) {
            HalClass::Operand
        } else {
            HalClass::Neither
        }
    }

    fn loc_node(&self, node: Node, st: &mut LocState) -> bool {
        let id = node.kind_id();
        if self.roles.comment_kinds.contains(&id) {
            let start = node.start_position().row;
            let mut end = node.end_position().row;
            // line_comment with a DocComment child: the doc comment includes the
            // trailing newline, so exclude the last line (rca's adjustment).
            if id == self.loc_line_comment
                && engine::loc::has_child_kind(node, self.loc_doc_comment)
            {
                end = end.saturating_sub(1);
            }
            engine::loc::add_cloc_lines(st, start, end);
            true
        } else {
            false
        }
    }
}

/// Parse `src` (already test-stripped) with tree-sitter-rust and compute metrics.
pub fn compute(src: &[u8]) -> Option<MetricInputs> {
    engine::compute(src, &*DIALECT)
}

/// Per-function metric units over each `function_item` subtree.
pub fn compute_functions(src: &[u8]) -> Vec<FunctionUnit> {
    engine::compute_functions(src, &*DIALECT)
}

#[cfg(test)]
#[path = "tests/dialect.rs"]
mod dialect_tests;
