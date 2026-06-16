use super::*;
use tree_sitter::Language;

/// `resolve_set` collects every id matching a `(name, is_named)` pair, and a
/// name absent from the grammar yields an empty contribution (not a panic).
#[test]
fn resolve_set_collects_named_and_anon() {
    let lang: Language = tree_sitter_rust::LANGUAGE.into();
    let ns = NameSet {
        named: vec!["function_item".to_string(), "does_not_exist".to_string()],
        anon: vec!["&&".to_string()],
    };
    let set = resolve_set(&lang, &ns);
    // function_item (named) and && (anon) resolve; the bogus name contributes 0.
    let func = lang.id_for_node_kind("function_item", true);
    let amp = lang.id_for_node_kind("&&", false);
    assert!(set.contains(&func));
    assert!(set.contains(&amp));
    assert_eq!(set.len(), 2);
}

/// An empty `NameSet` resolves to an empty set without scanning.
#[test]
fn resolve_set_empty_is_empty() {
    let lang: Language = tree_sitter_rust::LANGUAGE.into();
    assert!(resolve_set(&lang, &NameSet::default()).is_empty());
}

/// `resolve_one` finds the first matching id, distinguishing named vs anon, and
/// returns `u16::MAX` for an absent kind.
#[test]
fn resolve_one_distinguishes_named_and_absent() {
    let lang: Language = tree_sitter_rust::LANGUAGE.into();
    let named = resolve_one(
        &lang,
        &OneEntry {
            kind: "function_item".to_string(),
            named: true,
        },
    );
    assert_eq!(named, lang.id_for_node_kind("function_item", true));
    let absent = resolve_one(
        &lang,
        &OneEntry {
            kind: "definitely_not_a_kind".to_string(),
            named: true,
        },
    );
    assert_eq!(absent, u16::MAX);
}
