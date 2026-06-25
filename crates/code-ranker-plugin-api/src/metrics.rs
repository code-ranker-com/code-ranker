//! The plugin↔orchestrator **metric contract**: the raw tier-1 counts a language
//! plugin measures for one unit ([`MetricInputs`]) and a sub-file unit carrying
//! them ([`FunctionUnit`]).
//!
//! These are pure data. A plugin's metric engine *produces* them; the
//! orchestrator (`code-ranker-graph`) *consumes* them — running the tier-2 CEL
//! registry from `metrics/builtin.toml` and writing every metric onto the node.
//! They live here in the foundation crate so a plugin depends only on this API,
//! never on the heavier graph/enrichment crate, to hand back its measurements.

/// Raw tier-1 counts a per-language engine measures for one unit (a file or, for
/// the `functions` level, a function). Every tier-2 metric is a pure function of
/// these, evaluated by the orchestrator's built-in registry — see
/// `code-ranker-graph`'s `metrics/builtin.toml`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetricInputs {
    /// Halstead base counts (η₁/η₂/N₁/N₂), as floats (counts are small integers).
    pub eta1: f64,
    pub eta2: f64,
    pub n1: f64,
    pub n2: f64,
    /// Structural counts.
    pub spaces: f64,
    pub branches: f64,
    pub cognitive: f64,
    pub exits: f64,
    pub args: f64,
    pub closures: f64,
    /// LOC breakdown.
    pub sloc: f64,
    pub lloc: f64,
    pub cloc: f64,
    pub blank: f64,
    pub tloc: f64,
    /// Unit span sloc (`end_row − start_row`) — an MI input, not emitted itself.
    pub span_sloc: f64,
}

/// One sub-file unit (a function / method / closure) with its tier-1 counts.
/// Produced by a language engine's `compute_functions` for the optional
/// `functions` level. `kind` is a free-form, per-language string
/// (`fn` / `method` / `closure` / `lambda` / …).
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionUnit {
    pub kind: String,
    pub name: String,
    /// 1-based inclusive line span.
    pub start_line: u32,
    pub end_line: u32,
    pub inputs: MetricInputs,
}
