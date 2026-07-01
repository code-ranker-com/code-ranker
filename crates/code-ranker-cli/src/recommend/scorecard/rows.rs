//! The scorecard's **worst-modules** list: ranking file nodes by a metric lens or
//! by breach severity, plus the empty-list diagnostic and the row renderer. The
//! per-principle table stays in the parent module.

use crate::recommend::{
    FocusPaths, attr_short, clean_path, fmt_val, in_cycle, in_focus, is_internal, num, reco_for,
    thresholds_for, top_cycle_groups,
};
use code_ranker_graph::level_graph::LevelGraph;
use code_ranker_plugin_api::node::Node;

/// One metric (or cycle) breach on a node, with its tier.
struct Breach {
    metric: String,
    warning: bool,
    /// `value / threshold` — how far over the line (for picking the worst metric).
    ratio: f64,
    value: f64,
}

/// The severity tag shown on a worst-modules row. In the unfocused view it is the
/// node's worst breach; in the metric-focus view it is the focused metric's own
/// tier for that node — `Below` when the value is under the threshold (or the
/// metric has none), so a ranked-but-not-breaching module is labeled honestly
/// rather than always shown as `warn`.
#[derive(Clone, Copy)]
enum Tier {
    Warn,
    Info,
    Below,
}

impl Tier {
    fn label(self) -> &'static str {
        match self {
            Tier::Warn => "warn",
            Tier::Info => "info",
            Tier::Below => "—",
        }
    }
}

/// One row of the worst-modules list.
pub(super) struct ModRow {
    tier: Tier,
    path: String,
    head: String,
    rest: Vec<String>,
    n_warn: usize,
    n_info: usize,
    hk: f64,
}

/// Every selected-tier threshold a node breaches, plus cycle membership (treated
/// as a warning-tier signal — a cycle is always a real problem).
fn node_breaches(
    level: &LevelGraph,
    node: &Node,
    want_warning: bool,
    want_info: bool,
) -> Vec<Breach> {
    let mut out = Vec::new();
    for (metric, spec) in &level.node_attributes {
        let Some(th) = spec.thresholds else { continue };
        let Some(v) = num(node, metric) else { continue };
        if v > th.warning && want_warning {
            out.push(Breach {
                metric: metric.clone(),
                warning: true,
                ratio: if th.warning > 0.0 {
                    v / th.warning
                } else {
                    f64::INFINITY
                },
                value: v,
            });
        } else if v > th.info && want_info {
            out.push(Breach {
                metric: metric.clone(),
                warning: false,
                ratio: if th.info > 0.0 {
                    v / th.info
                } else {
                    f64::INFINITY
                },
                value: v,
            });
        }
    }
    if want_warning && in_cycle(node) {
        out.push(Breach {
            metric: "cycle".into(),
            warning: true,
            ratio: 1.0,
            value: 0.0,
        });
    }
    out
}

/// The line printed when the worst-modules list is empty. A plain `(none)` for a
/// genuine "no modules" result; a distinct diagnostic when a `--focus-path` filter
/// excluded *all* modules — so a wrong-form path (which used to render an identical
/// `(none)`) is no longer mistaken for "the code is clean".
pub(super) fn empty_modules_note(
    level: &LevelGraph,
    focus_paths: &FocusPaths,
    narrow: Option<&str>,
    want_warning: bool,
    want_info: bool,
) -> String {
    // Unscoped, genuinely empty, or a cycle view (which `--focus-path` never
    // narrows): the empty list is real, not a filter artifact.
    if focus_paths.is_empty() || narrow == Some("cycle") {
        return "  (none)\n".to_string();
    }
    // Re-rank WITHOUT the path filter: if that is also empty the list is genuinely
    // empty; otherwise the filter matched 0 of N and we say so.
    let empty = FocusPaths::new(&[], "");
    let unfiltered = match narrow {
        Some(m) => metric_mod_rows(level, m, usize::MAX, &empty),
        None => breach_mod_rows(level, want_warning, want_info, usize::MAX, &empty),
    };
    if unfiltered.is_empty() {
        return "  (none)\n".to_string();
    }
    let hint = unfiltered
        .first()
        .and_then(|r| r.path.split('/').next())
        .filter(|s| !s.is_empty())
        .unwrap_or("src");
    format!(
        "  (no module matched --focus-path — 0 of {} ranked modules.\n   \
         Reported locations look like '{hint}/…'; did you mean --focus-path '{hint}'?  \
         Run once without --focus-path to see the exact form.)\n",
        unfiltered.len()
    )
}

/// Worst-modules rows when narrowed to one metric (or the `cycle` pseudo-metric).
/// May push an explanatory heading line into `out` (the cycle branch does).
pub(super) fn narrowed_mod_rows(
    out: &mut String,
    level: &LevelGraph,
    m: &str,
    top: Option<usize>,
    limit: usize,
    focus_paths: &FocusPaths,
) -> Vec<ModRow> {
    if m == "cycle" {
        // A cycle is a global unit, so `--focus-path` does not narrow its members.
        cycle_mod_rows(out, level, top)
    } else {
        metric_mod_rows(level, m, limit, focus_paths)
    }
}

/// Cycle-narrowed worst-modules rows: list every member of each selected cycle
/// (so the whole loop is visible). Pushes the explanatory heading into `out`.
fn cycle_mod_rows(out: &mut String, level: &LevelGraph, top: Option<usize>) -> Vec<ModRow> {
    // ADP: `--top` counts CYCLES (default 1 — the biggest chain).
    let groups = top_cycle_groups(level, top.unwrap_or(1));
    match groups.as_slice() {
        [(g, members)] => out.push_str(&format!(
            "  one cycle ({}, {} modules) — all members listed; fix one cycle at a \
             time (avoid --top 2+):\n",
            g.kind,
            members.len()
        )),
        _ => out.push_str(&format!("  {} cycles — all members listed:\n", groups.len())),
    }
    let mut mod_rows: Vec<ModRow> = Vec::new();
    for (g, members) in &groups {
        for n in members {
            mod_rows.push(ModRow {
                tier: Tier::Warn,
                path: clean_path(&n.id),
                head: g.kind.clone(),
                rest: Vec::new(),
                n_warn: 0,
                n_info: 0,
                hk: num(n, "hk").unwrap_or(0.0),
            });
        }
    }
    mod_rows
}

/// Metric-narrowed worst-modules rows: the metric's ranked modules, capped,
/// restricted to `--focus-path` (empty = no restriction).
fn metric_mod_rows(level: &LevelGraph, m: &str, limit: usize, focus_paths: &FocusPaths) -> Vec<ModRow> {
    let reco = reco_for(level, m);
    // The focused metric's own tiers: tag each ranked module by where its value
    // actually lands (warn > warning, info > info), `Below` otherwise — never a
    // blanket `warn`. A metric with no configured threshold → every row `Below`.
    let th = thresholds_for(level, m);
    reco.sorted
        .iter()
        .filter(|n| in_focus(n, focus_paths))
        .take(limit)
        .map(|n| {
            let v = num(n, m);
            let head = match v {
                Some(v) if v != 0.0 => {
                    format!("{} {}", attr_short(level, m), fmt_val(v))
                }
                _ => attr_short(level, m).to_string(),
            };
            let value = v.unwrap_or(0.0);
            let tier = match th {
                Some(t) if value > t.warning => Tier::Warn,
                Some(t) if value > t.info => Tier::Info,
                _ => Tier::Below,
            };
            ModRow {
                tier,
                path: clean_path(&n.id),
                head,
                rest: Vec::new(),
                n_warn: 0,
                n_info: 0,
                hk: num(n, "hk").unwrap_or(0.0),
            }
        })
        .collect()
}

/// Worst-modules rows for the unnarrowed view: every internal node with a breach
/// in the selected tiers, ranked by warning/info counts then `hk`, truncated.
pub(super) fn breach_mod_rows(
    level: &LevelGraph,
    want_warning: bool,
    want_info: bool,
    limit: usize,
    focus_paths: &FocusPaths,
) -> Vec<ModRow> {
    let mut mod_rows: Vec<ModRow> = Vec::new();
    for n in level
        .nodes
        .iter()
        .filter(|n| is_internal(n) && in_focus(n, focus_paths))
    {
        let breaches = node_breaches(level, n, want_warning, want_info);
        if breaches.is_empty() {
            continue;
        }
        mod_rows.push(breach_row(level, n, &breaches));
    }
    mod_rows.sort_by(|a, b| {
        b.n_warn
            .cmp(&a.n_warn)
            .then(b.n_info.cmp(&a.n_info))
            .then(b.hk.total_cmp(&a.hk))
    });
    mod_rows.truncate(limit);
    mod_rows
}

/// Build the worst-modules row for one node from its (non-empty) breach list:
/// headline the worst metric (largest over-threshold ratio) and tag the rest.
fn breach_row(level: &LevelGraph, n: &Node, breaches: &[Breach]) -> ModRow {
    let n_warn = breaches.iter().filter(|b| b.warning).count();
    let n_info = breaches.iter().filter(|b| !b.warning).count();
    // Worst metric = the largest over-threshold ratio.
    let worst = breaches
        .iter()
        .max_by(|a, b| a.ratio.total_cmp(&b.ratio))
        .unwrap();
    let head = breach_label(level, &worst.metric, Some(worst.value));
    let rest: Vec<String> = breaches
        .iter()
        .filter(|b| b.metric != worst.metric)
        .map(|b| breach_label(level, &b.metric, None))
        .collect();
    ModRow {
        tier: if n_warn > 0 { Tier::Warn } else { Tier::Info },
        path: clean_path(&n.id),
        head,
        rest,
        n_warn,
        n_info,
        hk: num(n, "hk").unwrap_or(0.0),
    }
}

/// Short label for one breached metric: `"cycle"` for the cycle pseudo-metric,
/// else the metric's short name, optionally suffixed with its formatted value.
fn breach_label(level: &LevelGraph, metric: &str, value: Option<f64>) -> String {
    if metric == "cycle" {
        return "cycle".to_string();
    }
    match value {
        Some(v) => format!("{} {}", attr_short(level, metric), fmt_val(v)),
        None => attr_short(level, metric).to_string(),
    }
}

/// Render the worst-modules list (one numbered line per row) into `out`.
pub(super) fn render_mod_rows(out: &mut String, mod_rows: &[ModRow]) {
    let path_w = mod_rows.iter().map(|r| r.path.len()).max().unwrap_or(0);
    for (i, r) in mod_rows.iter().enumerate() {
        let tier = r.tier.label();
        let mut line = format!("{:>2} {:<4} {:<path_w$}  {}", i + 1, tier, r.path, r.head);
        if !r.rest.is_empty() {
            line.push_str(&format!("  +{}", r.rest.join(", ")));
        }
        out.push_str(&line);
        out.push('\n');
    }
}
