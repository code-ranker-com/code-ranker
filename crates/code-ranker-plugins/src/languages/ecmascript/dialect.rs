//! ECMAScript [`Dialect`] (JavaScript / TypeScript / TSX) for the generic engine.
//!
//! The walk logic lives in `crate::engine`; this is the ECMAScript-specific
//! layer: the caller's grammar (js → tree-sitter-javascript, ts/tsx →
//! tree-sitter-typescript), the resolved [`Roles`] (from `ecmascript/config.toml`), and
//! the genuinely-divergent predicates — chiefly the context-aware function vs
//! closure classification by ancestor walk (`is_func`/`is_closure`), the
//! per-grammar `else-if` rule (`else_if_via_else_clause`), and the cognitive
//! state machine. Unlike Rust/Python the engine collects ALL ids matching a
//! `(name, is_named)` pair (rca's duplicate `Identifier2`/`String2`/… variants).

use crate::engine::{self, CogCtx, CogState, Dialect, RoleCfg, Roles, UnitKind};
use code_ranker_plugin_api::metrics::{FunctionUnit, MetricInputs};
use std::collections::HashSet;
use std::sync::LazyLock;
use tree_sitter::{Language, Node};

static ROLE_CFG: LazyLock<RoleCfg> = LazyLock::new(|| {
    super::cfg::CONFIG
        .clone()
        .try_into()
        .expect("ecmascript/config.toml [roles]/[halstead]/[loc] parse")
});

/// The function-unit `kind` id strings from `[units]`, resolved once. The
/// classification LOGIC (by node kind) stays in `fn_kind`; only the emitted ids
/// are data.
static UNITS: LazyLock<std::collections::BTreeMap<String, String>> =
    LazyLock::new(|| crate::config::units(&super::cfg::CONFIG));

struct EcmaDialect {
    lang: Language,
    roles: Roles,
    // function-unit `kind` id strings (cloned from `UNITS` per construction).
    unit_method: String,
    unit_arrow: String,
    unit_generator: String,
    unit_default: String,
    /// rca's `is_else_if` differs by grammar: TypeScript checks the parent is an
    /// `else_clause`; JavaScript and TSX check the parent is an `if_statement`.
    else_if_via_else_clause: bool,
    // singleton ids used by the func/closure classification + cognitive nesting
    function_declaration: u16,
    method_definition: u16,
    function_expression: u16,
    arrow_function: u16,
    generator_function: u16,
    generator_function_declaration: u16,
    // ancestor / sibling / child sets (the `[roles.group]` sets)
    func_assign_anc: HashSet<u16>,
    func_stop: HashSet<u16>,
    arrow_assign_anc: HashSet<u16>,
    arrow_stop: HashSet<u16>,
    identifier: HashSet<u16>,
    property_identifier: HashSet<u16>,
    if_statement: HashSet<u16>,
    else_clause: HashSet<u16>,
    // cognitive
    expression_statement: HashSet<u16>,
    unary_expression: HashSet<u16>,
    binary_expression: HashSet<u16>,
    amp_amp: HashSet<u16>,
    pipe_pipe: HashSet<u16>,
    kw_else: HashSet<u16>,
}

impl EcmaDialect {
    fn new(lang: Language, else_if_via_else_clause: bool) -> Self {
        let roles = Roles::resolve(&lang, &ROLE_CFG);
        let one = |k: &str| roles.one(k);
        let g = |k: &str| roles.group(k).clone();
        let unit = |k: &str| UNITS.get(k).cloned().expect("[units] key");
        EcmaDialect {
            unit_method: unit("method"),
            unit_arrow: unit("arrow"),
            unit_generator: unit("generator"),
            unit_default: unit("default"),
            else_if_via_else_clause,
            function_declaration: one("function_declaration"),
            method_definition: one("method_definition"),
            function_expression: one("function_expression"),
            arrow_function: one("arrow_function"),
            generator_function: one("generator_function"),
            generator_function_declaration: one("generator_function_declaration"),
            func_assign_anc: g("func_assign_anc"),
            func_stop: g("func_stop"),
            arrow_assign_anc: g("arrow_assign_anc"),
            arrow_stop: g("arrow_stop"),
            identifier: g("identifier"),
            property_identifier: g("property_identifier"),
            if_statement: g("if_statement"),
            else_clause: g("else_clause"),
            expression_statement: g("expression_statement"),
            unary_expression: g("unary_expression"),
            binary_expression: g("binary_expression"),
            amp_amp: g("amp_amp"),
            pipe_pipe: g("pipe_pipe"),
            kw_else: g("kw_else"),
            lang,
            roles,
        }
    }

    fn is_else_if(&self, node: Node) -> bool {
        if !self.if_statement.contains(&node.kind_id()) {
            return false;
        }
        let want = if self.else_if_via_else_clause {
            &self.else_clause
        } else {
            &self.if_statement
        };
        node.parent().is_some_and(|p| want.contains(&p.kind_id()))
    }

    /// rca `count_specific_ancestors`: walk parents; stop at a `stop` node; count
    /// `check` nodes that aren't else-ifs.
    fn count_ancestors(&self, node: Node, check: &HashSet<u16>, stop: &HashSet<u16>) -> usize {
        let mut count = 0;
        let mut cur = node;
        while let Some(p) = cur.parent() {
            if stop.contains(&p.kind_id()) {
                break;
            }
            if check.contains(&p.kind_id()) && !self.is_else_if(p) {
                count += 1;
            }
            cur = p;
        }
        count
    }

    fn check_if_func(&self, node: Node) -> bool {
        self.count_ancestors(node, &self.func_assign_anc, &self.func_stop) > 0
            || is_child(node, &self.identifier)
    }
    fn check_if_arrow_func(&self, node: Node) -> bool {
        self.count_ancestors(node, &self.arrow_assign_anc, &self.arrow_stop) > 0
            || has_sibling(node, &self.property_identifier)
    }

    fn is_func(&self, node: Node) -> bool {
        let id = node.kind_id();
        if id == self.function_declaration || id == self.method_definition {
            true
        } else if id == self.function_expression {
            self.check_if_func(node)
        } else if id == self.arrow_function {
            self.check_if_arrow_func(node)
        } else {
            false
        }
    }
    fn is_closure(&self, node: Node) -> bool {
        let id = node.kind_id();
        if id == self.generator_function || id == self.generator_function_declaration {
            true
        } else if id == self.function_expression {
            !self.check_if_func(node)
        } else if id == self.arrow_function {
            !self.check_if_arrow_func(node)
        } else {
            false
        }
    }
}

fn is_child(node: Node, set: &HashSet<u16>) -> bool {
    let mut cur = node.walk();
    node.children(&mut cur).any(|c| set.contains(&c.kind_id()))
}

fn has_sibling(node: Node, set: &HashSet<u16>) -> bool {
    node.parent().is_some_and(|p| {
        let mut cur = p.walk();
        p.children(&mut cur).any(|c| set.contains(&c.kind_id()))
    })
}

impl Dialect for EcmaDialect {
    fn language(&self) -> &Language {
        &self.lang
    }
    fn roles(&self) -> &Roles {
        &self.roles
    }

    fn file_initial_spaces(&self) -> u32 {
        0 // `program` (the unit) is in `space_kinds`, so the walk counts it.
    }

    fn classify_unit(&self, node: Node) -> Option<UnitKind> {
        if self.is_func(node) {
            Some(UnitKind::Func)
        } else if self.is_closure(node) {
            Some(UnitKind::Closure)
        } else {
            None
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

        if self.if_statement.contains(&id) {
            if !self.is_else_if(node) {
                st.structural += nesting + depth + lambda + 1;
                cn = nesting + 1;
                st.boolean_op = None;
            }
        } else if self.roles.cog_nest.contains(&id) {
            st.structural += nesting + depth + lambda + 1;
            cn = nesting + 1;
            st.boolean_op = None;
        } else if self.kw_else.contains(&id) {
            st.structural += 1;
        } else if self.expression_statement.contains(&id) {
            st.boolean_op = None;
        } else if self.unary_expression.contains(&id) {
            st.boolean_op = Some(id);
        } else if self.binary_expression.contains(&id) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                let cid = child.kind_id();
                if self.amp_amp.contains(&cid) || self.pipe_pipe.contains(&cid) {
                    st.eval_boolean(cid);
                }
            }
        } else if id == self.function_declaration {
            cn = 0;
            cl = 0;
            if engine::has_ancestor_id(node, self.function_declaration) {
                cd = depth + 1;
            }
        } else if id == self.arrow_function {
            cl = lambda + 1;
        }

        CogCtx {
            nesting: cn,
            depth: cd,
            lambda: cl,
        }
    }

    fn is_function_unit(&self, node: Node) -> bool {
        let id = node.kind_id();
        // Excludes the `program` and `class` func-spaces (scoping, not functions).
        id == self.function_declaration
            || id == self.function_expression
            || id == self.arrow_function
            || id == self.method_definition
            || id == self.generator_function
            || id == self.generator_function_declaration
    }

    fn fn_kind(&self, node: Node) -> &str {
        let id = node.kind_id();
        if id == self.method_definition {
            &self.unit_method
        } else if id == self.arrow_function {
            &self.unit_arrow
        } else if id == self.generator_function || id == self.generator_function_declaration {
            &self.unit_generator
        } else {
            &self.unit_default
        }
    }

    // Halstead: no context exceptions — the default `hal_classify` (operators /
    // operands role sets) is correct for ECMAScript.

    // LOC: no special-cases — the default (noop / comment / statement / code-line
    // role sets) is correct for ECMAScript.
}

/// `else_if_via_else_clause`: true for TypeScript, false for JavaScript and TSX.
pub fn compute(src: &[u8], lang: &Language, else_if_via_else_clause: bool) -> Option<MetricInputs> {
    let d = EcmaDialect::new(lang.clone(), else_if_via_else_clause);
    engine::compute(src, &d)
}

/// Per-function metric units over each function-like subtree.
pub fn compute_functions(
    src: &[u8],
    lang: &Language,
    else_if_via_else_clause: bool,
) -> Vec<FunctionUnit> {
    let d = EcmaDialect::new(lang.clone(), else_if_via_else_clause);
    engine::compute_functions(src, &d)
}

#[cfg(test)]
#[path = "tests/dialect.rs"]
mod dialect_tests;
