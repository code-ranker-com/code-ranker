//! The `docs <subject>` command: print a reference doc to stdout. No analysis —
//! it resolves the merged config (auto-discovered from the current directory) and
//! the language plugin best-effort, then builds the principle + metric + category
//! specs from config + plugin (the same specs an analyzed snapshot carries, minus
//! the graph). Subjects:
//!   - `ai`          → the offline AI-agent playbook (resolved plugin → full
//!     playbook + catalog; none → a brief intro + plugin-setup guidance);
//!   - `metrics`     → an index of every metric, grouped by category;
//!   - `principles`  → an index of every design principle;
//!   - `<category>`  → that category (`loc`, `complexity`, …) + its member metrics;
//!   - `<metric>`    → that metric's spec card, plus its full doc when one exists;
//!   - `<principle>` → its full doc (or a synthetic card for a doc-less custom one);
//!   - anything else (or no subject) → a friendly catalog of every subject.
//!
//! Categories and metrics are read from the spec dictionaries; principle ids and
//! custom metrics declared in the project config (`[principles.<ID>]` /
//! `[metrics.<key>]`) are first-class subjects too.

use anyhow::{Result, bail};
use code_ranker_graph::version::CONFIG_VERSION;
use code_ranker_plugin_api::Principle;
use code_ranker_plugin_api::level::{AttributeGroup, AttributeSpec};
use code_ranker_plugin_api::plugin::PluginInput;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::config::{self, TemplatesConfig};
use crate::{plugin, templates};

/// Everything the `docs` subjects render, built from config + plugin with no graph.
struct DocSpecs {
    principles: Vec<Principle>,
    /// Metric/coupling specs by key (central catalog ⊕ plugin refinement ⊕ project
    /// `[metrics.<key>]`).
    node_attributes: BTreeMap<String, AttributeSpec>,
    /// Category (group) label/description by key.
    groups: BTreeMap<String, AttributeGroup>,
    templates: TemplatesConfig,
}

/// Print the doc for `subject` (or the catalog when it is absent / unknown).
pub(crate) fn run(
    subject: Option<&str>,
    plugin_arg: Option<&str>,
    config_entries: &[String],
) -> Result<()> {
    // `ai` is special: it needs no specs and has its own resolved/unresolved modes.
    if subject.is_some_and(|s| templates::normalize_id(s) == "ai") {
        return run_ai(plugin_arg, config_entries);
    }

    let specs = build_specs(plugin_arg, config_entries);

    let Some(subject) = subject else {
        // Bare `docs`: the catalog is the help, so exit 0.
        print!(
            "{}",
            templates::with_trailing_newline(render_catalog(&specs, None))
        );
        return Ok(());
    };

    // Every subject is matched on its normalized form (case/separator-insensitive),
    // so `fan_in`, `Fan-in`, and `FAN in` all resolve the same metric.
    let want = templates::normalize_id(subject);
    if want == "metrics" {
        emit(render_metrics_index(&specs));
    } else if want == "principles" {
        emit(render_principles_index(&specs));
    } else if let Some(cat) = category_key(&specs, subject) {
        emit(render_category(&specs, &cat));
    } else if let Some(p) = specs
        .principles
        .iter()
        .find(|p| templates::normalize_id(&p.id) == want)
    {
        emit(render_principle(&specs, &p.id)?);
    } else if let Some(key) = specs
        .node_attributes
        .keys()
        .find(|k| templates::normalize_id(k) == want)
    {
        emit(render_metric(&specs, key));
    } else {
        // Unknown subject: print the catalog so the caller sees every option, then
        // fail (non-zero) — it was a real lookup miss, not a help request.
        emit(render_catalog(&specs, Some(subject)));
        bail!("unknown docs subject {subject:?} — see the list above");
    }
    Ok(())
}

fn emit(md: String) {
    print!("{}", templates::with_trailing_newline(md));
}

/// The `docs ai` playbook: resolve the plugin best-effort (like the rest of `docs`,
/// from `.`), then serve the full playbook or, when none resolves, the intro + a
/// filled-in *Select a language* template.
fn run_ai(plugin_arg: Option<&str>, config_entries: &[String]) -> Result<()> {
    let input = Path::new(".");
    let cfg_plugin = config::load(input, config_entries, &[], &[], &[])
        .ok()
        .and_then(|loaded| loaded.config.plugin);
    let md = match plugin::resolve_plugin(plugin_arg, cfg_plugin.as_deref(), input) {
        Ok(_) => templates::ai_doc()?,
        Err(reason) => fill_select(&templates::ai_doc_intro()?, &reason.to_string()),
    };
    emit(md);
    Ok(())
}

/// Fill the *Select a language* template (authored in `base/AI.md`) with the live
/// values: the resolver diagnostic, the built-in plugin names, the config version.
fn fill_select(intro: &str, reason: &str) -> String {
    intro
        .replace("{reason}", reason)
        .replace("{plugins}", &plugin::names())
        .replace("{config_version}", CONFIG_VERSION)
}

/// Build the doc specs from config + plugin, no analysis. Best-effort throughout:
/// a missing config yields the built-in defaults, a *broken* one is ignored (the
/// central catalog + plugin still answer most subjects), and an unresolved plugin
/// just drops the language-specific principles/refinements.
fn build_specs(plugin_arg: Option<&str>, config_entries: &[String]) -> DocSpecs {
    let input = Path::new(".");

    // Central, language-neutral metric specs + their category groups.
    let (default_metric_specs, metric_groups) = code_ranker_graph::metric_specs();
    let (coupling_specs, coupling_groups) = code_ranker_graph::coupling_specs();
    let mut groups = BTreeMap::new();
    groups.extend(metric_groups);
    groups.extend(coupling_groups);

    let cfg = config::load(input, config_entries, &[], &[], &[])
        .ok()
        .map(|loaded| loaded.config);

    let cfg_plugin = cfg.as_ref().and_then(|c| c.plugin.clone());
    let plugin_name = plugin::resolve_plugin(plugin_arg, cfg_plugin.as_deref(), input).ok();

    let pinput = cfg
        .as_ref()
        .map_or_else(default_plugin_input, |c| PluginInput {
            ignore: c.ignore.paths.clone(),
            ignore_tests: c.ignore.tests,
            gitignore: c.ignore.gitignore,
            ignore_files: c.ignore.ignore_files,
            hidden: c.ignore.hidden,
        });

    // Metrics: central catalog refined by the active plugin, plus coupling and the
    // project's node-scope declarative metrics (built-ins win a key collision).
    let metric_specs = match &plugin_name {
        Some(n) => plugin::metric_specs(n, default_metric_specs),
        None => default_metric_specs,
    };
    let mut node_attributes: BTreeMap<String, AttributeSpec> = BTreeMap::new();
    node_attributes.extend(metric_specs);
    node_attributes.extend(coupling_specs);
    if let Some(c) = &cfg {
        for (k, d) in &c.metrics {
            if d.scope == code_ranker_graph::Scope::Node {
                node_attributes
                    .entry(k.clone())
                    .or_insert_with(|| d.to_attribute_spec());
            }
        }
    }

    // Principles: plugin catalog overlaid with the project's `[principles.<ID>]`.
    let catalog = match &plugin_name {
        Some(n) => plugin::principles(n, &pinput),
        None => Vec::new(),
    };
    let principles = match &cfg {
        Some(c) => config::merge_project_principles(catalog, &c.principles),
        None => catalog,
    };

    let templates = cfg.map(|c| c.templates).unwrap_or_default();

    DocSpecs {
        principles,
        node_attributes,
        groups,
        templates,
    }
}

/// A neutral `PluginInput` for the no-config fallback (a broken config file). The
/// principle/metric-spec hooks barely read these, so the defaults only affect the
/// rare degraded path.
fn default_plugin_input() -> PluginInput {
    PluginInput {
        ignore: Vec::new(),
        ignore_tests: true,
        gitignore: true,
        ignore_files: true,
        hidden: false,
    }
}

// ── subject resolution helpers ────────────────────────────────────────────────

/// Every category key: the defined groups plus any `group` a metric spec references
/// (a metric may name a category that ships no `[categories.<key>]` label).
fn category_keys(specs: &DocSpecs) -> BTreeSet<String> {
    let mut keys: BTreeSet<String> = specs.groups.keys().cloned().collect();
    for spec in specs.node_attributes.values() {
        if let Some(g) = &spec.group {
            keys.insert(g.clone());
        }
    }
    keys
}

/// The canonical category key matching `subject` (separator/case-insensitive), if any.
fn category_key(specs: &DocSpecs, subject: &str) -> Option<String> {
    let want = templates::normalize_id(subject);
    category_keys(specs)
        .into_iter()
        .find(|k| templates::normalize_id(k) == want)
}

/// The metrics in one category, by key (sorted — `BTreeMap` order).
fn metrics_in_category<'a>(specs: &'a DocSpecs, key: &str) -> Vec<(&'a String, &'a AttributeSpec)> {
    specs
        .node_attributes
        .iter()
        .filter(|(_, s)| s.group.as_deref() == Some(key))
        .collect()
}

// ── rendering ─────────────────────────────────────────────────────────────────

/// A metric's display name: `name` › `label` › the key itself.
fn metric_name<'a>(spec: &'a AttributeSpec, key: &'a str) -> &'a str {
    spec.name
        .as_deref()
        .or(spec.label.as_deref())
        .unwrap_or(key)
}

/// The first line of a (possibly multi-line, `<br>`-encoded) description.
fn one_line(desc: &str) -> &str {
    desc.split("<br>").next().unwrap_or(desc).trim()
}

/// A category's label (› its key) and optional description.
fn category_label(specs: &DocSpecs, key: &str) -> String {
    specs
        .groups
        .get(key)
        .and_then(|g| g.label.clone())
        .unwrap_or_else(|| key.to_string())
}

/// Strip a leading `ID — ` from a principle title so the listing column is tight.
fn principle_title(p: &Principle) -> &str {
    p.title
        .split_once(" — ")
        .map(|(_, rest)| rest)
        .unwrap_or(&p.title)
}

/// The categories section shared by `docs metrics` and the catalog: each category
/// header (`key: Label — description`) followed by its member metrics.
fn categories_block(specs: &DocSpecs) -> String {
    let mut out = String::new();
    let cats = category_keys(specs);
    for key in &cats {
        out.push_str(&format!("\n  {key}: {}", category_label(specs, key)));
        if let Some(d) = specs.groups.get(key).and_then(|g| g.description.as_deref()) {
            out.push_str(&format!(" — {d}"));
        }
        out.push('\n');
        for (k, spec) in metrics_in_category(specs, key) {
            out.push_str(&format!("    - {k}: {}\n", metric_name(spec, k)));
        }
    }
    // Metrics with no category (e.g. the categorical `cycle`): list them too.
    let uncategorized: Vec<_> = specs
        .node_attributes
        .iter()
        .filter(|(_, s)| s.group.is_none())
        .collect();
    if !uncategorized.is_empty() {
        out.push_str("\n  (uncategorized)\n");
        for (k, spec) in uncategorized {
            out.push_str(&format!("    - {k}: {}\n", metric_name(spec, k)));
        }
    }
    out
}

/// The principles section shared by `docs principles` and the catalog.
fn principles_block(specs: &DocSpecs) -> String {
    if specs.principles.is_empty() {
        return "  (none — no language plugin resolved here)\n".to_string();
    }
    specs
        .principles
        .iter()
        .map(|p| format!("  - {}: {}\n", p.id, principle_title(p)))
        .collect()
}

/// `docs metrics`: every metric, grouped by category.
fn render_metrics_index(specs: &DocSpecs) -> String {
    format!(
        "Metrics — print one with `code-ranker docs <metric>`:\n{}",
        categories_block(specs)
    )
}

/// `docs principles`: every design principle.
fn render_principles_index(specs: &DocSpecs) -> String {
    format!(
        "Principles — print one with `code-ranker docs <ID>`:\n\n{}",
        principles_block(specs)
    )
}

/// `docs <category>`: the category label + description + its member metrics.
fn render_category(specs: &DocSpecs, key: &str) -> String {
    let mut out = format!("{key}: {}", category_label(specs, key));
    if let Some(d) = specs.groups.get(key).and_then(|g| g.description.as_deref()) {
        out.push_str(&format!("\n{d}"));
    }
    out.push_str("\n\nMetrics — print one with `code-ranker docs <metric>`:\n");
    for (k, spec) in metrics_in_category(specs, key) {
        out.push_str(&format!("  - {k}: {}", metric_name(spec, k)));
        if let Some(d) = spec.description.as_deref() {
            out.push_str(&format!(" — {}", one_line(d)));
        }
        out.push('\n');
    }
    out
}

/// `docs <metric>`: the spec card (label / name / category / description / formula),
/// then the full prose doc appended when one exists (e.g. `hk` → `HK.md`).
fn render_metric(specs: &DocSpecs, subject: &str) -> String {
    let (key, spec) = specs
        .node_attributes
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(subject))
        .expect("caller checked the key exists");
    let name = metric_name(spec, key);
    let mut out = format!("# {key}: {name}");
    if let Some(short) = spec.short.as_deref().filter(|s| *s != name) {
        out.push_str(&format!(" ({short})"));
    }
    out.push('\n');
    if let Some(g) = &spec.group {
        out.push_str(&format!("\nCategory: {g} — {}\n", category_label(specs, g)));
    }
    if let Some(d) = spec.description.as_deref() {
        out.push_str(&format!("\n{}\n", d.replace("<br>", "\n")));
    }
    if let Some(f) = &spec.formula {
        out.push_str(&format!("\nFormula: {f}\n"));
    }
    // A metric whose `remediation` points at a corpus doc (e.g. `hk` → `HK.md`)
    // gets that full doc appended — so `docs hk` is the complete reference.
    if let Ok(prose) = templates::resolve_doc_from_specs(
        &specs.principles,
        &specs.node_attributes,
        &specs.templates,
        key,
    ) {
        out.push_str(&format!("\n---\n\n{}\n", prose.trim_end()));
    }
    out
}

/// `docs <principle>`: the full prose doc, or — for a project-defined principle with
/// no doc file — a synthetic card from its title / sort-metric / prompt.
fn render_principle(specs: &DocSpecs, subject: &str) -> Result<String> {
    match templates::resolve_doc_from_specs(
        &specs.principles,
        &specs.node_attributes,
        &specs.templates,
        subject,
    ) {
        Ok(md) => Ok(md),
        Err(_) => {
            let p = specs
                .principles
                .iter()
                .find(|p| p.id.eq_ignore_ascii_case(subject))
                .expect("caller checked the principle exists");
            let mut out = format!(
                "# {}: {}\n\nSort metric: `{}`\n",
                p.id, p.title, p.sort_metric
            );
            if !p.prompt.is_empty() {
                out.push_str(&format!("\n{}\n", p.prompt));
            }
            Ok(out)
        }
    }
}

/// The catalog of every subject — shown for a bare `docs` (help) and, with a lead
/// note, for an unknown subject. A uniform two-level tree: each group (a metric
/// category, then `principles`) on its own line, its members indented beneath. Every
/// name on every line — group or member — is itself a valid `docs <subject>`.
fn render_catalog(specs: &DocSpecs, unknown: Option<&str>) -> String {
    let mut out = String::new();
    if let Some(s) = unknown {
        out.push_str(&format!("Unknown docs subject `{s}`.\n\n"));
    }
    out.push_str("code-ranker docs <subject> — print a reference doc to stdout (no analysis).\n");
    out.push_str(&categories_block(specs));
    // Principles render as one more group, exactly like a metric category.
    out.push_str("\n  principles: Design principles\n");
    out.push_str(
        &specs
            .principles
            .iter()
            .map(|p| format!("    - {}: {}\n", p.id, principle_title(p)))
            .collect::<String>(),
    );
    out.push_str(
        "\nCall `docs` with any name above — e.g. `docs principles`, `docs KISS`, \
         `docs cloc`, `docs complexity`. Also `docs ai` (the agent playbook) and \
         `docs metrics` (the full metric index).\n",
    );
    out
}

#[cfg(test)]
#[path = "docs_test.rs"]
mod tests;
