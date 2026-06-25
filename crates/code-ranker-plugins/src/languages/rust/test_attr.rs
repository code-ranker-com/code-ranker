//! Shared `#[test]` / `#[cfg(test)]` attribute detection.
//!
//! Both the metric test-stripper (`strip.rs`) and the module-graph walk
//! (`module_graph/walk.rs`) need to recognise a test-gated item; the predicate
//! lives here once so the two never drift. The `test` / `bench` / `cfg`
//! attribute idents are DATA (`[syn]` in `config.toml`); the LOGIC stays here.

/// True if an attribute gates an item to tests: `#[test]`, `#[bench]`, or a
/// `cfg(...)` whose predicate contains a bare `test` identifier (`#[cfg(test)]`,
/// `#[cfg(all(test, …))]`, `#[cfg(any(test, …))]`). A `test` **identifier**
/// inside `cfg(...)` is what matches — `cfg(feature = "test")` (a string
/// literal) does not.
pub(super) fn is_test_attr(attr: &syn::Attribute) -> bool {
    if attr.path().is_ident(super::cfg::SYN_TEST.as_str())
        || attr.path().is_ident(super::cfg::SYN_BENCH.as_str())
    {
        return true;
    }
    if attr.path().is_ident(super::cfg::SYN_CFG.as_str())
        && let syn::Meta::List(list) = &attr.meta
    {
        return tokens_have_test_ident(list.tokens.clone());
    }
    false
}

/// Recursively scan a token stream for a bare `test` identifier (descends into
/// `all(...)` / `any(...)` / `not(...)` groups).
pub(super) fn tokens_have_test_ident(ts: proc_macro2::TokenStream) -> bool {
    ts.into_iter().any(|t| match t {
        proc_macro2::TokenTree::Ident(i) => i == super::cfg::SYN_TEST.as_str(),
        proc_macro2::TokenTree::Group(g) => tokens_have_test_ident(g.stream()),
        _ => false,
    })
}
