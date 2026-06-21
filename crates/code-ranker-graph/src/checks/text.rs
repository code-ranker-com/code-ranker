//! Text helpers for custom checks: whole-word identifier scanning (used by
//! `[rules.defs]` expansion and predicate-variable detection) and `{key}`
//! message interpolation over a node's values.
//!
//! Every function here is a leaf — it depends only on `Node` / `AttrValue` and
//! the path helpers, never on a `checks.rs`-local item — so the parent module
//! imports it one way and no parent↔child cycle forms.

use crate::nodepath::{node_path, split_path};
use code_ranker_plugin_api::{attrs::AttrValue, node::Node};

/// Whole-word membership: does `haystack` reference identifier `word` (not as a
/// substring of a larger identifier)? Mirrors the metric registry's scan.
pub(super) fn references(haystack: &str, word: &str) -> bool {
    word_positions(haystack, word).next().is_some()
}

/// Replace every whole-word occurrence of `word` in `s` with `repl` (UTF-8 safe;
/// only ASCII-identifier boundaries are considered, so string-literal contents
/// are copied through unchanged).
pub(super) fn replace_word(s: &str, word: &str, repl: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for start in word_positions(s, word) {
        out.push_str(&s[last..start]);
        out.push_str(repl);
        last = start + word.len();
    }
    out.push_str(&s[last..]);
    out
}

/// Byte offsets of every whole-word occurrence of `word` in `s`.
fn word_positions<'a>(s: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    let bytes = s.as_bytes();
    let is_word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut from = 0;
    std::iter::from_fn(move || {
        while let Some(rel) = s[from..].find(word) {
            let start = from + rel;
            let end = start + word.len();
            from = start + 1;
            let before_ok = start == 0 || !is_word(bytes[start - 1]);
            let after_ok = end == bytes.len() || !is_word(bytes[end]);
            if before_ok && after_ok {
                return Some(start);
            }
        }
        None
    })
}

/// Fill `{key}` placeholders in a message from the node's values. `{` / `}` are
/// ASCII, so byte offsets from `find` stay on char boundaries. An unmatched `{`
/// or an unknown key is left verbatim.
pub(super) fn render_message(template: &str, node: &Node) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let key = &after[..close];
                match lookup_value(node, key) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('{');
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Resolve a `{key}` for message interpolation — a derived path field or any
/// node attribute, formatted as a human string.
fn lookup_value(node: &Node, key: &str) -> Option<String> {
    match key {
        "path" => Some(node_path(node)),
        "name" | "stem" | "ext" | "dir" => {
            let parts = split_path(&node_path(node));
            Some(match key {
                "name" => parts.name,
                "stem" => parts.stem,
                "ext" => parts.ext,
                _ => parts.dir,
            })
        }
        _ => node.attrs.get(key).map(format_attr),
    }
}

/// Human form of an attribute value: integers and whole floats print without a
/// decimal point; fractional floats keep two places.
fn format_attr(value: &AttrValue) -> String {
    match value {
        AttrValue::Int(i) => i.to_string(),
        AttrValue::Float(f) if f.fract() == 0.0 => format!("{}", *f as i64),
        AttrValue::Float(f) => format!("{f:.2}"),
        AttrValue::Bool(b) => b.to_string(),
        AttrValue::Str(s) => s.clone(),
    }
}
