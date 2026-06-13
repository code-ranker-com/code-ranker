//! Central, language-agnostic complexity pass. Given a structural graph whose
//! file nodes carry their absolute path as `id`, this reads each file, picks a
//! `rust-code-analysis` parser by extension, and writes the metrics into the
//! node's `attrs` as flat keys. It is the single place that knows
//! rust-code-analysis; plugins emit structure only.
//!
//! The metric attribute dictionary it can produce is exposed via
//! [`metric_specs`] so the orchestrator can declare it in the snapshot.

use code_ranker_graph::attrs::num_attr;
use code_ranker_plugin_api::{
    attrs::ValueType,
    graph::Graph,
    level::{AttributeGroup, AttributeSpec, Direction, SpecRow, attr_dict, group},
};
use rust_code_analysis::{
    FuncSpace, JavascriptParser, ParserTrait, PythonParser, RustParser, TsxParser,
    TypescriptParser, metrics,
};
use std::collections::BTreeMap;
use std::path::Path;

/// Annotate every file node (`kind == "file"`) whose `id` is a readable source
/// file of a known extension with complexity metrics. Returns the number of
/// nodes annotated. Nodes whose file cannot be read/parsed are left untouched.
pub fn annotate(graph: &mut Graph) -> usize {
    let mut annotated = 0usize;
    for node in &mut graph.nodes {
        if node.kind != "file" {
            continue;
        }
        let path = Path::new(&node.id);
        let Ok(src) = std::fs::read(path) else {
            continue;
        };
        let Some((space, tloc)) = parse_metrics(path, src) else {
            continue;
        };
        write_metrics(node, &space, tloc);
        annotated += 1;
    }
    annotated
}

/// True if any attribute gates an item to tests: `#[test]`, `#[bench]`, or
/// `#[cfg(test)]` / `#[cfg(all(test, …))]` / `#[cfg(any(test, …))]`. A `test`
/// **identifier** inside `cfg(...)` is what matches — `cfg(feature = "test")`
/// (a string literal) does not.
fn is_test_attr(attr: &syn::Attribute) -> bool {
    if attr.path().is_ident("test") || attr.path().is_ident("bench") {
        return true;
    }
    if attr.path().is_ident("cfg")
        && let syn::Meta::List(list) = &attr.meta
    {
        return tokens_have_test_ident(list.tokens.clone());
    }
    false
}

/// Recursively scan a token stream for a bare `test` identifier (descends into
/// `all(...)` / `any(...)` groups).
fn tokens_have_test_ident(ts: proc_macro2::TokenStream) -> bool {
    ts.into_iter().any(|t| match t {
        proc_macro2::TokenTree::Ident(i) => i == "test",
        proc_macro2::TokenTree::Group(g) => tokens_have_test_ident(g.stream()),
        _ => false,
    })
}

/// Visitor collecting the 1-based, inclusive line ranges of test-only items
/// (`#[cfg(test)]` modules, `#[test]`/`#[cfg(test)]` fns), attribute line
/// included. It recurses into ordinary modules to catch nested test modules but
/// not into a test item it already captured.
#[derive(Default)]
struct TestSpans {
    ranges: Vec<(usize, usize)>,
}

impl TestSpans {
    fn record(&mut self, attrs: &[syn::Attribute], span: proc_macro2::Span) {
        use syn::spanned::Spanned;
        let start = attrs
            .iter()
            .map(|a| a.span().start().line)
            .chain(std::iter::once(span.start().line))
            .min()
            .unwrap_or(0);
        self.ranges.push((start, span.end().line));
    }
}

impl<'ast> syn::visit::Visit<'ast> for TestSpans {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        use syn::spanned::Spanned;
        if m.attrs.iter().any(is_test_attr) {
            self.record(&m.attrs, m.span());
        } else {
            syn::visit::visit_item_mod(self, m);
        }
    }
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        use syn::spanned::Spanned;
        if f.attrs.iter().any(is_test_attr) {
            self.record(&f.attrs, f.span());
        }
    }
}

/// Step 1 of the Rust line accounting: remove `#[cfg(test)]` / `#[test]` /
/// `#[bench]` items so the production metrics (`sloc` / `cloc` / `blank` / `hk` /
/// complexity) are then measured on production code only. Returns the production
/// source **and** `tloc` — the number of test lines removed (the whole test
/// region: attribute, body, braces). Parse failures or no test items return the
/// source unchanged with `tloc = 0`.
fn strip_cfg_test(src: &[u8]) -> (Vec<u8>, usize) {
    use syn::visit::Visit;
    let Ok(text) = std::str::from_utf8(src) else {
        return (src.to_vec(), 0);
    };
    let Ok(file) = syn::parse_file(text) else {
        return (src.to_vec(), 0);
    };
    let mut spans = TestSpans::default();
    spans.visit_file(&file);
    if spans.ranges.is_empty() {
        return (src.to_vec(), 0);
    }
    let drop: std::collections::HashSet<usize> =
        spans.ranges.iter().flat_map(|&(s, e)| s..=e).collect();
    let tloc = drop.len();
    let mut out: String = text
        .lines()
        .enumerate()
        .filter(|(i, _)| !drop.contains(&(i + 1)))
        .map(|(_, l)| l)
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    (out.into_bytes(), tloc)
}

/// Pick a parser by file extension and compute the file's production `FuncSpace`
/// plus `tloc` — the number of **test** lines (`#[cfg(test)]` / `#[test]` /
/// `#[bench]`) removed before measuring. Only Rust strips tests, so `tloc` is
/// `0.0` for every other language. (Step 1 strips tests; step 2, in
/// `write_metrics`, counts sloc/cloc/blank on the production remainder.)
fn parse_metrics(path: &Path, src: Vec<u8>) -> Option<(FuncSpace, f64)> {
    let ext = path.extension().and_then(|e| e.to_str())?;
    match ext {
        "rs" => {
            let (prod_src, tloc) = strip_cfg_test(&src);
            let prod = metrics(&RustParser::new(prod_src, path, None), path)?;
            Some((prod, tloc as f64))
        }
        "py" => metrics(&PythonParser::new(src, path, None), path).map(|s| (s, 0.0)),
        "ts" | "mts" | "cts" => {
            metrics(&TypescriptParser::new(src, path, None), path).map(|s| (s, 0.0))
        }
        "tsx" => metrics(&TsxParser::new(src, path, None), path).map(|s| (s, 0.0)),
        "js" | "jsx" | "mjs" | "cjs" => {
            metrics(&JavascriptParser::new(src, path, None), path).map(|s| (s, 0.0))
        }
        _ => None,
    }
}

/// The value at which a per-file metric carries no signal and is **omitted** from
/// output (see [`code_ranker_plugin_api::level::AttributeSpec::omit_at`]). `0` for
/// almost everything; `1` for `cyclomatic` — McCabe counts the single
/// straight-line path even for branch-free code, so a function-less file would
/// otherwise report a vacuous `1`. [`write_metrics`] gates on this value and
/// [`metric_specs`] publishes the same value on each spec, so the two never drift.
fn metric_omit_at(key: &str) -> f64 {
    match key {
        "cyclomatic" => 1.0,
        _ => 0.0,
    }
}

/// Write the metric attributes for one file node. Each value is omitted at its
/// `omit_at` (0 for most metrics, 1 for `cyclomatic`); the LOC block is
/// additionally gated on `sloc > 0` and the Halstead block on `volume > 0`.
fn write_metrics(node: &mut code_ranker_plugin_api::node::Node, s: &FuncSpace, tloc: f64) {
    let m = &s.metrics;
    let mut put = |key: &str, v: f64| {
        let a = num_attr(v);
        // Drop the metric when it sits at its no-signal value (`omit_at`): absent
        // from the JSON, blank in the viewer. `0` for almost everything; `1` for
        // `cyclomatic`, whose floor is `1` — so a function-less file omits it
        // rather than showing a meaningless `1`. The same per-key value is
        // published on the spec, so the frontend knows what an absent cell means.
        if a == num_attr(metric_omit_at(key)) {
            node.attrs.remove(key);
        } else {
            node.attrs.insert(key.to_string(), a);
        }
    };

    // `cyclomatic()` / `cognitive()` return only the ROOT space's own value —
    // for a file that is a constant 1 (no top-level branching) and 0
    // respectively. The real complexity lives in the child function spaces, so we
    // read the aggregated `*_sum` — the file's total complexity over its
    // functions. A function-less file sums to the `omit_at` floor (cyclomatic 1,
    // cognitive 0) and is dropped by `put`.
    put("cyclomatic", m.cyclomatic.cyclomatic_sum());
    put("cognitive", m.cognitive.cognitive_sum());
    // Like cyclomatic/cognitive, these are per-function counts: the root space's
    // own value is 0 for a file (the real counts live in the child function
    // spaces), so read the aggregated `*_sum` — never the root accessor.
    put("exits", m.nexits.exit_sum());
    put("args", m.nargs.fn_args_sum() + m.nargs.closure_args_sum());
    put("closures", m.nom.closures_sum());

    put("mi", m.mi.mi_original());
    put("mi_sei", m.mi.mi_sei());

    // `sloc` here means *physical lines of code* — lines with real code, excluding
    // blanks and comment-only lines (see this key's spec). rust-code-analysis names
    // that `ploc()`; its `sloc()` is the total line count (already exposed as `loc`).
    //
    // NOTE: for Rust these four — `sloc` (physical), `lloc` (logical), `cloc`
    // (comments), `blank` — are all measured on the *production* source, i.e.
    // AFTER `strip_cfg_test` removed `#[cfg(test)]` / `#[test]` / `#[bench]`
    // items. So none of them count lines from inline tests; those go to `tloc`.
    let sloc = m.loc.ploc();
    if sloc > 0.0 {
        put("sloc", sloc);
        put("lloc", m.loc.lloc());
        put("cloc", m.loc.cloc());
        put("blank", m.loc.blank());
    }
    // Test source lines (`#[cfg(test)]`/`#[test]`/`#[bench]`), the complement of
    // `sloc`. Zero (non-Rust, or no inline tests) is dropped by `put`.
    put("tloc", tloc);

    let volume = m.halstead.volume();
    if volume > 0.0 {
        put("length", m.halstead.length());
        put(
            "vocabulary",
            m.halstead.u_operators() + m.halstead.u_operands(),
        );
        put("volume", volume);
        put("effort", m.halstead.effort());
        put("time", m.halstead.time());
        put("bugs", m.halstead.bugs());
    }
}

/// The complexity metric attribute dictionary and its groups, fully enriched
/// (label/name/short/description/formula/calc/direction) so the UI hardcodes no
/// metric. The orchestrator merges these into each level's `node_attributes` /
/// `attribute_groups` (then prunes to keys actually present) and overlays
/// language thresholds. Coupling/cycle specs live in `code-ranker-graph`.
pub fn metric_specs() -> (
    BTreeMap<String, AttributeSpec>,
    BTreeMap<String, AttributeGroup>,
) {
    use Direction::{HigherBetter, LowerBetter};
    use ValueType::Float;
    let mut specs = attr_dict(vec![
        (
            "cyclomatic",
            SpecRow {
                group: "complexity",
                label: "Cyclomatic",
                name: "Cyclomatic complexity",
                short: "Cyclomatic",
                description: "Number of independent paths through the code — roughly the minimum number of test cases needed to cover every branch.<br>A function starts at 1 and gains +1 per decision point: each `if` / `else if`, every `match` / `switch` arm, every loop, and each `&&` / `||` in a condition.<br>Summed across every function in the file, so it grows with both size and branching — the file's total branching burden.<br>Counts paths only, ignoring how deeply they nest. For a readability-weighted view see `cognitive`.",
                formula: "Σ (branches + 1) over functions",
                direction: LowerBetter,
                ..Default::default()
            },
        ),
        (
            "cognitive",
            SpecRow {
                group: "complexity",
                label: "Cognitive",
                name: "Cognitive complexity",
                short: "Cognitive",
                description: "How hard the code is for a human to follow — not just how many paths it has.<br>Like `cyclomatic` it adds +1 for each break in linear flow (`if`, `else`, `match`, loops, `catch`, chained `&&` / `||`), but it also adds an extra +1 for every level of nesting: an `if` inside a loop inside an `if` costs far more than three flat `if`s.<br>That nesting penalty is the point — deeply indented logic is what actually strains a reader, so a high `cognitive` next to a modest `cyclomatic` flags tangled, hard-to-read code.<br>Summed across every function in the file.",
                direction: LowerBetter,
                ..Default::default()
            },
        ),
        (
            "exits",
            SpecRow {
                group: "complexity",
                label: "Exits",
                name: "Exit points",
                short: "Exits",
                description: "Number of exit points (return/throw) in the unit.",
                direction: LowerBetter,
                ..Default::default()
            },
        ),
        (
            "args",
            SpecRow {
                group: "complexity",
                label: "Args",
                name: "Arguments",
                short: "Args",
                description: "Number of function / closure arguments.",
                direction: LowerBetter,
                ..Default::default()
            },
        ),
        (
            "closures",
            SpecRow {
                group: "complexity",
                label: "Closures",
                name: "Closures",
                short: "Closures",
                description: "Number of closures defined in the unit.",
                direction: LowerBetter,
                ..Default::default()
            },
        ),
        (
            "mi",
            SpecRow {
                group: "maintainability",
                value_type: Float,
                label: "MI",
                name: "Maintainability index",
                short: "MI",
                description: "Maintainability Index (0–100, higher is more maintainable). Derived from Halstead volume, cyclomatic complexity, and SLOC.",
                formula: "171 − 5.2·ln(volume) − 0.23·cyclomatic − 16.2·ln(sloc)",
                direction: HigherBetter,
                ..Default::default()
            },
        ),
        (
            "mi_sei",
            SpecRow {
                group: "maintainability",
                value_type: Float,
                label: "MI (SEI)",
                name: "Maintainability (SEI)",
                short: "MI SEI",
                description: "SEI variant of the Maintainability Index — adds a bonus for comment density.",
                formula: "MI + 50·sin(√(2.4 × comment-ratio))",
                direction: HigherBetter,
                ..Default::default()
            },
        ),
        (
            "sloc",
            SpecRow {
                group: "loc",
                label: "Source",
                name: "Source lines",
                short: "SLOC",
                description: "Source lines of code — lines with at least one non-whitespace, non-comment character. Blank and comment-only lines are not counted. In Rust, lines inside `#[cfg(test)]` / `#[test]` items are excluded too, so this counts production code only (unlike `loc`, the raw file line count).",
                ..Default::default()
            },
        ),
        (
            "lloc",
            SpecRow {
                group: "loc",
                label: "Logical",
                name: "Logical lines",
                short: "Logical",
                description: "Logical lines — counts statements, not physical lines. In Rust, measured on production code only (inline `#[cfg(test)]` / `#[test]` tests are excluded, like `sloc`; their lines are `tloc`).",
                ..Default::default()
            },
        ),
        (
            "cloc",
            SpecRow {
                group: "loc",
                label: "Comments",
                name: "Comment lines",
                short: "Comments",
                description: "Comment-only lines (inline comments on code lines are not counted). In Rust, measured on production code only (inline `#[cfg(test)]` / `#[test]` tests are excluded, like `sloc`; their lines are `tloc`).",
                ..Default::default()
            },
        ),
        (
            "blank",
            SpecRow {
                group: "loc",
                label: "Blank",
                name: "Blank lines",
                short: "Blank",
                description: "Empty or whitespace-only lines. In Rust, measured on production code only (inline `#[cfg(test)]` / `#[test]` tests are excluded, like `sloc`; their lines are `tloc`).",
                ..Default::default()
            },
        ),
        (
            "tloc",
            SpecRow {
                group: "loc",
                label: "Test",
                name: "Test lines",
                short: "TLOC",
                description: "Test lines of code — the lines inside `#[cfg(test)]` / `#[test]` / `#[bench]` items (Rust), removed before the production metrics are measured. The complement of `sloc`: test code never inflates a file's size, HK, or complexity.",
                ..Default::default()
            },
        ),
        (
            "length",
            SpecRow {
                group: "halstead",
                value_type: Float,
                label: "Length",
                name: "Halstead length",
                short: "H.len",
                description: "Program length — total operator + operand occurrences.",
                formula: "N₁ + N₂",
                direction: LowerBetter,
                ..Default::default()
            },
        ),
        (
            "vocabulary",
            SpecRow {
                group: "halstead",
                value_type: Float,
                label: "Vocabulary",
                name: "Halstead vocabulary",
                short: "H.vocab",
                description: "Vocabulary — distinct operators + operands.",
                formula: "η₁ + η₂",
                direction: LowerBetter,
                ..Default::default()
            },
        ),
        (
            "volume",
            SpecRow {
                group: "halstead",
                value_type: Float,
                label: "Volume",
                name: "Halstead volume",
                short: "H.vol",
                description: "Algorithm size in bits, from distinct operators and operands.",
                formula: "length × log₂(vocabulary)",
                calc: "length * Math.log2(vocabulary)",
                direction: LowerBetter,
                ..Default::default()
            },
        ),
        (
            "effort",
            SpecRow {
                group: "halstead",
                value_type: Float,
                label: "Effort",
                name: "Halstead effort",
                short: "H.effort",
                description: "Mental effort to implement the algorithm.",
                formula: "volume × difficulty",
                direction: LowerBetter,
                ..Default::default()
            },
        ),
        (
            "time",
            SpecRow {
                group: "halstead",
                value_type: Float,
                label: "Time",
                name: "Halstead time, s",
                short: "H.time(s)",
                description: "Estimated implementation time, in seconds.",
                formula: "effort ÷ 18",
                calc: "effort / 18",
                direction: LowerBetter,
                ..Default::default()
            },
        ),
        (
            "bugs",
            SpecRow {
                group: "halstead",
                value_type: Float,
                label: "Bugs",
                name: "Halstead bugs",
                short: "H.bugs",
                description: "Estimated delivered bugs — a rough predictor of defect density.",
                formula: "effort^⅔ ÷ 3000",
                calc: "effort ** (2/3) / 3000",
                direction: LowerBetter,
                ..Default::default()
            },
        ),
    ]);
    // Publish each metric's no-signal value on its spec, from the same source
    // `write_metrics` gates on — so the emitted JSON and the declared spec agree.
    for (key, spec) in specs.iter_mut() {
        spec.omit_at = metric_omit_at(key);
    }
    let mut groups = BTreeMap::new();
    groups.insert(
        "complexity".to_string(),
        group("Complexity", "Code complexity metrics"),
    );
    groups.insert(
        "halstead".to_string(),
        group("Halstead", "Halstead software metrics"),
    );
    groups.insert(
        "loc".to_string(),
        group("Lines of Code", "Lines of code breakdown"),
    );
    groups.insert(
        "maintainability".to_string(),
        group("Maintainability", "Maintainability index"),
    );
    (specs, groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(src: &str) -> String {
        String::from_utf8(strip_cfg_test(src.as_bytes()).0).unwrap()
    }

    #[test]
    fn strips_cfg_test_module_with_its_attribute() {
        let out = strip(
            "pub fn prod() -> i32 {\n    1\n}\n\n\
             #[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() { assert_eq!(prod(), 1); }\n}\n",
        );
        assert!(out.contains("pub fn prod"), "production kept: {out}");
        assert!(!out.contains("mod tests"), "test mod removed: {out}");
        assert!(
            !out.contains("#[cfg(test)]"),
            "the cfg attr line removed too: {out}"
        );
        assert!(!out.contains("fn t()"), "test fn removed: {out}");
    }

    #[test]
    fn strips_standalone_test_and_bench_fns() {
        let out = strip("fn prod() {}\n#[test]\nfn it_works() {}\n#[bench]\nfn b(_: &mut ()) {}\n");
        assert!(out.contains("fn prod"));
        assert!(
            !out.contains("it_works") && !out.contains("fn b("),
            "test/bench fns removed: {out}"
        );
    }

    #[test]
    fn keeps_non_test_cfg_and_similarly_named_items() {
        // `cfg(feature = "test")` is a string literal, not a `test` ident; a
        // `mod tests_data` is not gated. Both stay.
        let out = strip("#[cfg(feature = \"test\")]\npub mod gated {}\npub mod tests_data {}\n");
        assert!(out.contains("pub mod gated"), "feature-cfg kept: {out}");
        assert!(
            out.contains("tests_data"),
            "non-gated lookalike kept: {out}"
        );
    }

    #[test]
    fn strips_cfg_all_test_combinations() {
        let out = strip("fn p() {}\n#[cfg(all(test, feature = \"x\"))]\nmod t {}\n");
        assert!(out.contains("fn p"));
        assert!(!out.contains("mod t"), "cfg(all(test,…)) removed: {out}");
    }

    #[test]
    fn unchanged_without_tests_or_on_parse_error() {
        let prod = "pub fn a() {}\n";
        assert_eq!(
            strip_cfg_test(prod.as_bytes()),
            (prod.as_bytes().to_vec(), 0)
        );
        let broken = "@@@ not rust @@@";
        assert_eq!(
            strip_cfg_test(broken.as_bytes()),
            (broken.as_bytes().to_vec(), 0)
        );
    }

    #[test]
    fn tloc_counts_the_whole_removed_test_region() {
        // 4 lines removed: the #[cfg(test)] attr, `mod tests {`, the body line,
        // and the closing `}`.
        let src = "pub fn p() {}\n#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n";
        let (_prod, tloc) = strip_cfg_test(src.as_bytes());
        assert_eq!(tloc, 4);
    }

    fn metric(node: &code_ranker_plugin_api::node::Node, key: &str) -> Option<f64> {
        match node.attrs.get(key) {
            Some(code_ranker_plugin_api::attrs::AttrValue::Int(v)) => Some(*v as f64),
            Some(code_ranker_plugin_api::attrs::AttrValue::Float(v)) => Some(*v),
            _ => None,
        }
    }

    #[test]
    fn per_function_metrics_aggregate_over_child_functions() {
        // Regression for the whole "root-vs-sum" class: `write_metrics` once read
        // the ROOT space value for `cyclomatic` / `cognitive` / `exits` / `args` /
        // `closures`, which for a file is the vacuous root count (0, or 1 for
        // cyclomatic) — every file looked identical. The real signal lives in the
        // child function spaces, so each must be the SUM over them.
        //
        // `a` takes 2 args, nests two `if`s, and `return`s; `b` defines a 1-arg
        // closure. So the file must surface: cyclomatic (summed branches), a
        // non-zero cognitive (nesting), exits (the `return`), args (2 fn + 1
        // closure = 3), and closures (1).
        let src = "fn a(x: i32, y: i32) -> i32 { if x > 0 { if x > 1 { return x; } y } else { 3 } }\n\
                   fn b() -> i32 { let f = |z: i32| z + 1; f(2) }\n";
        let (space, tloc) =
            parse_metrics(Path::new("t.rs"), src.as_bytes().to_vec()).expect("parses");
        let mut node = code_ranker_plugin_api::node::Node {
            id: "t.rs".into(),
            kind: "file".into(),
            name: "t.rs".into(),
            parent: None,
            attrs: Default::default(),
        };
        write_metrics(&mut node, &space, tloc);

        // Each is summed over the child functions — well above the vacuous root
        // value, proving aggregation rather than a root-only read.
        let cyc = metric(&node, "cyclomatic").expect("cyclomatic present");
        assert!(cyc > 1.0, "cyclomatic should be summed, got {cyc}");
        let cog = metric(&node, "cognitive").expect("cognitive present");
        assert!(cog > 0.0, "cognitive should be summed, got {cog}");
        let exits = metric(&node, "exits").expect("exits present");
        assert!(exits >= 1.0, "exits should count the `return`, got {exits}");
        let args = metric(&node, "args").expect("args present");
        assert!(
            args >= 3.0,
            "args should sum fn (2) + closure (1), got {args}"
        );
        let closures = metric(&node, "closures").expect("closures present");
        assert!(
            closures >= 1.0,
            "closures should count the closure, got {closures}"
        );
    }

    #[test]
    fn declaration_only_file_emits_no_complexity() {
        // No functions → only the root space → cyclomatic is a vacuous 1 and
        // cognitive is 0. Both must be dropped (not shown as a meaningless "1"),
        // matching how `put` already drops cognitive's 0. Mirrors real files like
        // a clap CLI model or a type-definitions module.
        let src = "pub struct Cli { pub verbose: bool }\n\
                   pub enum Mode { A, B }\n";
        let (space, tloc) =
            parse_metrics(Path::new("t.rs"), src.as_bytes().to_vec()).expect("parses");
        let mut node = code_ranker_plugin_api::node::Node {
            id: "t.rs".into(),
            kind: "file".into(),
            name: "t.rs".into(),
            parent: None,
            attrs: Default::default(),
        };
        write_metrics(&mut node, &space, tloc);

        assert!(
            metric(&node, "cyclomatic").is_none(),
            "a function-less file must not emit a vacuous cyclomatic"
        );
        assert!(
            metric(&node, "cognitive").is_none(),
            "a function-less file must not emit cognitive"
        );
    }
}
