//! Python [`Dialect`] for the generic metric engine.
//!
//! The walk logic lives in `crate::engine`; this is the thin Python-specific
//! layer: the grammar, the resolved [`Roles`] (from `python.toml`), and the few
//! predicates that differ for Python — the loop-`else` branch, the cognitive
//! state machine (`elif`/`else`/`finally`/`except` weights, lambda nesting), the
//! `string` docstring/operand distinction, and the `string` LOC special-case.

use crate::engine::{
    self, CogCtx, CogState, Dialect, HalClass, LocState, RoleCfg, Roles, UnitKind,
};
use code_ranker_graph::{FunctionUnit, MetricInputs};
use std::sync::LazyLock;
use tree_sitter::{Language, Node};

static CONFIG: LazyLock<toml::Table> =
    LazyLock::new(|| crate::config::load(include_str!("config.toml")));

static ROLE_CFG: LazyLock<RoleCfg> = LazyLock::new(|| {
    CONFIG
        .clone()
        .try_into()
        .expect("python.toml [roles]/[halstead]/[loc] parse")
});

struct PythonDialect {
    lang: Language,
    roles: Roles,
    // function-unit `kind` id strings from `[units]` (classification logic below).
    unit_method: String,
    unit_default: String,
    function_definition: u16,
    class_definition: u16,
    lambda: u16,
    // structural loop-else
    kw_else: u16,
    else_clause: u16,
    for_statement: u16,
    while_statement: u16,
    // cognitive
    if_statement: u16,
    conditional_expression: u16,
    elif_clause: u16,
    finally_clause: u16,
    except_clause: u16,
    expression_statement: u16,
    expression_list: u16,
    tuple: u16,
    not_operator: u16,
    boolean_operator: u16,
    kw_and: u16,
    kw_or: u16,
    // halstead / loc special
    hal_string: u16,
    hal_expression_statement: u16,
    loc_string: u16,
    loc_expression_statement: u16,
}

impl PythonDialect {
    fn new() -> Self {
        let lang: Language = tree_sitter_python::LANGUAGE.into();
        let roles = Roles::resolve(&lang, &ROLE_CFG);
        let one = |k: &str| roles.one(k);
        let sp = |k: &str| roles.special(k);
        let units = crate::config::units(&CONFIG);
        let unit = |k: &str| units.get(k).cloned().expect("[units] key");
        PythonDialect {
            unit_method: unit("method"),
            unit_default: unit("default"),
            function_definition: one("function_definition"),
            class_definition: one("class_definition"),
            lambda: one("lambda"),
            kw_else: one("kw_else"),
            else_clause: one("else_clause"),
            for_statement: one("for_statement"),
            while_statement: one("while_statement"),
            if_statement: one("if_statement"),
            conditional_expression: one("conditional_expression"),
            elif_clause: one("elif_clause"),
            finally_clause: one("finally_clause"),
            except_clause: one("except_clause"),
            expression_statement: one("expression_statement"),
            expression_list: one("expression_list"),
            tuple: one("tuple"),
            not_operator: one("not_operator"),
            boolean_operator: one("boolean_operator"),
            kw_and: one("kw_and"),
            kw_or: one("kw_or"),
            hal_string: sp("hal_string"),
            hal_expression_statement: sp("hal_expression_statement"),
            loc_string: sp("loc_string"),
            loc_expression_statement: sp("loc_expression_statement"),
            lang,
            roles,
        }
    }
}

static DIALECT: LazyLock<PythonDialect> = LazyLock::new(PythonDialect::new);

impl Dialect for PythonDialect {
    fn language(&self) -> &Language {
        &self.lang
    }
    fn roles(&self) -> &Roles {
        &self.roles
    }

    fn file_initial_spaces(&self) -> u32 {
        1 // the module (unit) space
    }

    fn classify_unit(&self, node: Node) -> Option<UnitKind> {
        let id = node.kind_id();
        if id == self.function_definition {
            Some(UnitKind::Func)
        } else if id == self.lambda {
            Some(UnitKind::Closure)
        } else {
            None
        }
    }

    fn extra_branches(&self, node: Node) -> u32 {
        // An `else` attached to a for/while loop counts (not an if's else).
        if node.kind_id() == self.kw_else
            && let Some(clause) = node.parent()
            && clause.kind_id() == self.else_clause
            && let Some(stmt) = clause.parent()
            && (stmt.kind_id() == self.for_statement || stmt.kind_id() == self.while_statement)
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

        if id == self.if_statement
            || id == self.for_statement
            || id == self.while_statement
            || id == self.conditional_expression
        {
            st.structural += nesting + depth + lambda + 1;
            cn = nesting + 1;
            st.boolean_op = None;
        } else if id == self.elif_clause {
            st.structural += 1;
            st.boolean_op = None;
        } else if id == self.else_clause || id == self.finally_clause {
            st.structural += 1;
        } else if id == self.except_clause {
            cn = nesting + 1;
            st.structural += cn + depth + lambda + 1; // rca: nesting+=1; increment (uses new nesting)
        } else if id == self.expression_statement || id == self.expression_list || id == self.tuple
        {
            st.boolean_op = None;
        } else if id == self.not_operator {
            st.boolean_op = Some(id);
        } else if id == self.boolean_operator {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                let cid = child.kind_id();
                if cid == self.kw_and || cid == self.kw_or {
                    st.eval_boolean(cid);
                }
            }
        } else if id == self.lambda {
            cl = lambda + 1;
        } else if id == self.function_definition
            && engine::has_ancestor_id(node, self.function_definition)
        {
            cd = depth + 1;
        }

        CogCtx {
            nesting: cn,
            depth: cd,
            lambda: cl,
        }
    }

    fn is_function_unit(&self, node: Node) -> bool {
        node.kind_id() == self.function_definition
    }

    fn fn_kind(&self, node: Node) -> &str {
        // `method` when the nearest enclosing scope is a class, else `function`.
        let mut p = node.parent();
        while let Some(n) = p {
            if n.kind_id() == self.class_definition {
                return &self.unit_method;
            }
            if n.kind_id() == self.function_definition {
                return &self.unit_default;
            }
            p = n.parent();
        }
        &self.unit_default
    }

    fn hal_classify(&self, node: Node) -> HalClass {
        let id = node.kind_id();
        if self.roles.operators.contains(&id) {
            HalClass::Operator
        } else if self.roles.operands.contains(&id) {
            HalClass::Operand
        } else if id == self.hal_string {
            // operand unless it is a bare docstring (sole child of expression_statement)
            let is_docstring = node.parent().is_some_and(|p| {
                p.kind_id() == self.hal_expression_statement && p.child_count() == 1
            });
            if is_docstring {
                HalClass::Neither
            } else {
                HalClass::Operand
            }
        } else {
            HalClass::Neither
        }
    }

    fn loc_node(&self, node: Node, st: &mut LocState) -> bool {
        let id = node.kind_id();
        if id != self.loc_string {
            return false;
        }
        // A bare docstring (sole child of an expression_statement) is a comment;
        // otherwise a string spanning past its parent's start line is code.
        let start = node.start_position().row;
        let end = node.end_position().row;
        if let Some(parent) = node.parent() {
            if parent.kind_id() == self.loc_expression_statement {
                engine::loc::add_cloc_lines(st, start, end);
            } else if parent.start_position().row != start {
                engine::loc::check_comment_ends_on_code_line(st, start);
                st.lines.insert(start);
            }
        }
        true
    }
}

/// Parse `src` with tree-sitter-python and compute the file-level metrics.
pub fn compute(src: &[u8]) -> Option<MetricInputs> {
    engine::compute(src, &*DIALECT)
}

/// Per-function metric units over each `function_definition` subtree.
pub fn compute_functions(src: &[u8]) -> Vec<FunctionUnit> {
    engine::compute_functions(src, &*DIALECT)
}

#[cfg(test)]
#[path = "tests/dialect.rs"]
mod dialect_tests;
