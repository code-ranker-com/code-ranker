//! Principle catalog and the `[specs.<key>]` description overrides a plugin applies
//! over the central `builtin.toml` attribute specs.

use code_ranker_plugin_api::Principle;
use code_ranker_plugin_api::level::AttributeSpec;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use toml::{Table, Value};

/// One `[[principles]]` entry as read from config. Mirrors the data shape of the
/// CLI's generic principle catalog; the plugin turns it into a
/// `code_ranker_plugin_api::Principle`, deriving `doc_url` from its `id`.
#[derive(Debug, Clone, Deserialize)]
pub struct PrincipleCfg {
    pub id: String,
    pub title: String,
    pub sort_metric: String,
    #[serde(default)]
    pub connections: Vec<String>,
    pub prompt: String,
}

/// One `[specs.<key>]` entry: per-language overrides applied over the central
/// `builtin.toml` attribute specs. Only the fields a language tweaks are set;
/// the rest are left untouched on the inherited spec.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SpecOverride {
    #[serde(default)]
    pub description: Option<String>,
}

/// Read the `[[principles]]` array from a merged config (empty if absent).
pub fn principles(cfg: &Table) -> Vec<PrincipleCfg> {
    cfg.get("principles")
        .cloned()
        .map(|v| v.try_into().expect("[[principles]] shape"))
        .unwrap_or_default()
}

/// Read a top-level string key from a merged config.
fn string_field<'a>(cfg: &'a Table, key: &str) -> Option<&'a str> {
    cfg.get(key)?.as_str()
}

/// Which principle ids a language overrides with its own corpus doc — the doc
/// analogue of the `defaults.toml ⊕ <lang>.toml` inheritance. Read from the
/// `doc_overrides` key of a merged config:
/// - `doc_overrides = "*"` → the language has its OWN doc for every principle
///   (a full corpus: rust / python / typescript).
/// - `doc_overrides = ["SRP", …]` → only these ids resolve to the language's own
///   folder; the rest fall back to the shared `base/` corpus.
/// - absent → the language has no own corpus; every doc resolves to `base/`.
enum DocOverrides {
    All,
    Ids(HashSet<String>),
    None,
}

impl DocOverrides {
    fn covers(&self, id: &str) -> bool {
        match self {
            DocOverrides::All => true,
            DocOverrides::Ids(ids) => ids.contains(id),
            DocOverrides::None => false,
        }
    }
}

fn doc_overrides(cfg: &Table) -> DocOverrides {
    match cfg.get("doc_overrides") {
        Some(Value::String(s)) if s == "*" => DocOverrides::All,
        Some(Value::Array(a)) => DocOverrides::Ids(
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
        ),
        _ => DocOverrides::None,
    }
}

/// Build the fully-resolved [`Principle`] list from a merged config: the common
/// catalog (from `defaults.toml`) plus any language-specific principles, in that
/// order (the merge-by-`id` already yields it). `label` is the `id`.
///
/// `doc_url` inherits from a shared `base/` corpus the same way config inherits
/// `defaults.toml`: it resolves to `{doc_base}/{doc_lang}/{id}.md` for the ids a
/// language overrides (`doc_overrides`), and to `{doc_base}/base/{id}.md` for the
/// rest. So a language without its own corpus (no `doc_overrides`) points every
/// link at `base/`, and a full-corpus language (`doc_overrides = "*"`) points
/// every link at its own folder. `doc_base` (the host/repo prefix, common) lives
/// in `defaults.toml`; if it is absent the `doc_url` is left `None`.
pub fn resolved_principles(cfg: &Table) -> Vec<Principle> {
    let base = string_field(cfg, "doc_base");
    let lang = string_field(cfg, "doc_lang");
    let overrides = doc_overrides(cfg);
    principles(cfg)
        .into_iter()
        .map(|p| Principle {
            doc_url: base.map(|b| {
                // Own folder for an overridden id (needs a `doc_lang`); `base/`
                // otherwise — the shared fallback corpus.
                let folder = match lang {
                    Some(l) if overrides.covers(&p.id) => l,
                    _ => "base",
                };
                format!("{b}/{folder}/{}.md", p.id)
            }),
            label: p.id.clone(),
            id: p.id,
            title: p.title,
            prompt: p.prompt,
            sort_metric: p.sort_metric,
            connections: p.connections,
        })
        .collect()
}

/// Read the `[specs]` table from a merged config as `key → override`
/// (empty if absent).
pub fn spec_overrides(cfg: &Table) -> BTreeMap<String, SpecOverride> {
    cfg.get("specs")
        .cloned()
        .map(|v| v.try_into().expect("[specs] shape"))
        .unwrap_or_default()
}

/// Apply a config's `[specs.<key>]` description overrides over the central
/// builtin metric specs — the shared body of every plugin's `metric_specs`. A
/// language refines a metric's description (e.g. enumerating the exact Halstead
/// operators/operands it counts) without restating the rest of the spec; an
/// override whose key isn't a known metric is ignored.
pub fn apply_spec_overrides(
    mut defaults: BTreeMap<String, AttributeSpec>,
    cfg: &Table,
) -> BTreeMap<String, AttributeSpec> {
    for (key, ov) in spec_overrides(cfg) {
        if let Some(spec) = defaults.get_mut(&key)
            && let Some(desc) = ov.description
        {
            spec.description = Some(desc);
        }
    }
    defaults
}
