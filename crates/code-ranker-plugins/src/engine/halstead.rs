//! Halstead base counts (η₁/η₂/N₁/N₂). The derivation lives in
//! `code-ranker-graph`; here we only count distinct/total operators and operands.
//!
//! Operator/operand classification is the dialect's `hal_classify` (the default
//! uses the `operators`/`operands` role sets; rust/python override it for the
//! few context exceptions). Operators dedup by kind id; operands dedup by text.

use super::core::{Dialect, HalClass, OpMap, OperandMap};
use tree_sitter::Node;

/// Halstead base counts.
pub struct Halstead {
    pub eta1: f64,
    pub eta2: f64,
    pub n1: f64,
    pub n2: f64,
}

pub fn compute<D: Dialect>(root: Node, src: &[u8], d: &D) -> Halstead {
    let mut operators: OpMap = OpMap::new();
    let mut operands: OperandMap = OperandMap::new();
    walk(root, src, d, &mut operators, &mut operands);

    let n1: u64 = operators.values().sum();
    let n2: u64 = operands.values().sum();
    Halstead {
        eta1: operators.len() as f64,
        eta2: operands.len() as f64,
        n1: n1 as f64,
        n2: n2 as f64,
    }
}

fn walk<D: Dialect>(
    node: Node,
    src: &[u8],
    d: &D,
    operators: &mut OpMap,
    operands: &mut OperandMap,
) {
    match d.hal_classify(node) {
        HalClass::Operator => {
            *operators.entry(node.kind_id()).or_insert(0) += 1;
        }
        HalClass::Operand => {
            let text = node.utf8_text(src).unwrap_or("").as_bytes().to_vec();
            *operands.entry(text).or_insert(0) += 1;
        }
        HalClass::Neither => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, src, d, operators, operands);
    }
}
