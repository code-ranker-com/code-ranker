//! Rust line accounting: strip inline `#[cfg(test)]` / `#[test]` / `#[bench]`
//! items so production metrics (`sloc` / `cloc` / `blank` / `hk` / complexity)
//! are measured on production code only, with the removed test lines counted as
//! `tloc`. The test-attribute predicate lives in `super::test_attr`.

use code_ranker_plugin_api::metrics::MetricInputs;

use super::test_attr::is_test_attr;

/// Measure Rust complexity metrics for one file from its source bytes.
/// `#[cfg(test)]` / `#[test]` / `#[bench]` items are stripped first (their lines
/// become `tloc`), then the generic engine via the rust dialect runs. Returns the
/// measured [`MetricInputs`] (`None` if the source did not parse); the orchestrator
/// writes them onto the node.
pub(super) fn rust_file_metrics(src: &[u8]) -> Option<MetricInputs> {
    let (prod, tloc) = strip_cfg_test(src);
    let mut m = super::dialect::compute(&prod)?;
    m.tloc = tloc as f64;
    Some(m)
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
pub(super) fn strip_cfg_test(src: &[u8]) -> (Vec<u8>, usize) {
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
