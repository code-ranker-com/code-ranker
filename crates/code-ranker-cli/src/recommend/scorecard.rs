//! The console triage scorecard behind the `scorecard` report format — a
//! per-principle table mirroring the viewer's per-principle badges, plus the worst
//! modules overall.

mod rows;

use rows::{breach_mod_rows, empty_modules_note, narrowed_mod_rows, render_mod_rows};

use super::{FocusPaths, Severity, attr_short, clean_path, file_count, fmt_val, num, reco_for};
use anyhow::Result;
use code_ranker_graph::level_graph::LevelGraph;
use code_ranker_plugin_api::Principle;

/// One row of the per-principle table.
struct Row {
    id: String,
    name: String,
    warn: usize,
    info: usize,
    top: String,
}

/// Render the console triage scorecard: a per-principle table (warning/info
/// counts + the worst module) followed by the worst modules overall, then a hint
/// pointing at `--prompt <ID>` for a specific principle/metric.
pub fn render_scorecard(
    plugin: &str,
    level: &LevelGraph,
    principles: &[Principle],
    severities: &[Severity],
    top: Option<usize>,
    focus: Option<&super::Focus>,
    focus_paths: &FocusPaths,
) -> Result<String> {
    let want_warning = severities
        .iter()
        .any(|s| matches!(s, Severity::Warning | Severity::Auto));
    let want_info = severities
        .iter()
        .any(|s| matches!(s, Severity::Info | Severity::Auto));

    // `--focus` picks the lens. A metric frames the scorecard by that metric alone
    // (no principle rows — the worst-modules list carries the ranking); a principle
    // shows just that principle's row; without it, the full per-principle triage. The
    // metric the worst-modules list ranks by is the focused metric, the focused
    // principle's `sort_metric`, or none (a breach-ranked list).
    let (shown_principles, narrow): (Vec<&Principle>, Option<&str>) = match focus {
        Some(super::Focus::Metric(m)) => (Vec::new(), Some(m.as_str())),
        Some(super::Focus::Principle(id)) => {
            let p: Vec<&Principle> = principles.iter().filter(|p| &p.id == id).collect();
            let m = p.first().map(|p| p.sort_metric.as_str());
            (p, m)
        }
        None => (principles.iter().collect(), None),
    };

    let mut out = String::new();
    out.push_str(&format!(
        "scorecard  ({plugin}, {} files)\n\n",
        file_count(level)
    ));
    // A metric lens names what it is focused on (there is no principle row to).
    if let Some(super::Focus::Metric(m)) = focus {
        out.push_str(&format!("focus: {}\n", metric_focus_label(level, m)));
    }

    // ── Per-principle table ──────────────────────────────────────────────────
    let mut rows = principle_rows(level, &shown_principles, narrow, want_warning, want_info);
    rows.sort_by(|a, b| b.warn.cmp(&a.warn).then(b.info.cmp(&a.info)));

    if rows.is_empty() && focus.is_none() {
        out.push_str("No threshold breaches for the selected severity.\n");
        return Ok(out);
    }

    // The per-principle table (skipped under a metric lens — the worst-modules list
    // below carries the ranking instead).
    if !rows.is_empty() {
        render_principle_table(&mut out, &rows, want_warning, want_info);
    }

    // ── Worst modules ────────────────────────────────────────────────────────
    out.push_str("\nWORST MODULES\n");
    let limit = top.unwrap_or(15);

    let mod_rows = match narrow {
        // Focused on a metric: that metric's ranked modules (may emit a heading).
        Some(m) => narrowed_mod_rows(&mut out, level, m, top, limit, focus_paths),
        // Otherwise: every internal node with a breach, ranked by severity.
        None => breach_mod_rows(level, want_warning, want_info, limit, focus_paths),
    };

    if mod_rows.is_empty() {
        out.push_str(&empty_modules_note(
            level,
            focus_paths,
            narrow,
            want_warning,
            want_info,
        ));
    } else {
        render_mod_rows(&mut out, &mod_rows);
    }

    // ── Next-step hint ───────────────────────────────────────────────────────
    // Pin `--plugins <lang>` so the fix-prompt targets the same language this
    // scorecard is for (a multi-language repo would otherwise re-resolve it).
    out.push_str(&format!(
        "\n→ code-ranker report . --plugins {plugin} --prompt <PRINCIPLE|METRIC>   (AI fix-prompt to stdout)\n"
    ));

    Ok(out)
}

/// The metric lens's header label: `HK — Henry–Kafura` (short/label + `name`),
/// or just the key when no richer names exist.
fn metric_focus_label(level: &LevelGraph, m: &str) -> String {
    if m == "cycle" {
        return "cycle — dependency cycles".to_string();
    }
    let spec = level.node_attributes.get(m);
    let label = spec
        .and_then(|s| s.short.as_deref().or(s.label.as_deref()))
        .unwrap_or(m);
    match spec.and_then(|s| s.name.as_deref()) {
        Some(n) if n != label => format!("{label} — {n}"),
        _ => label.to_string(),
    }
}

/// Build the per-principle table rows from the shown principles.
fn principle_rows(
    level: &LevelGraph,
    shown_principles: &[&Principle],
    narrow: Option<&str>,
    want_warning: bool,
    want_info: bool,
) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for p in shown_principles {
        let reco = reco_for(level, &p.sort_metric);
        // Skip principles with nothing in the selected tiers (unless narrowed).
        let in_scope =
            (want_warning && reco.warning_count > 0) || (want_info && reco.info_count > 0);
        if narrow.is_none() && !in_scope {
            continue;
        }
        let top_module = principle_top_module(level, p, &reco);
        rows.push(Row {
            id: p.id.clone(),
            // Strip a leading "ID — " from the title to keep the column short.
            name: p
                .title
                .split_once(" — ")
                .map(|(_, rest)| rest)
                .unwrap_or(&p.title)
                .to_string(),
            warn: reco.warning_count,
            info: reco.info_count,
            top: top_module,
        });
    }
    rows
}

/// The "top module" cell for a principle row: the worst-ranked module under the
/// principle's metric, annotated with the metric value (or `(cycle)` / a bare path).
fn principle_top_module(level: &LevelGraph, p: &Principle, reco: &super::Reco) -> String {
    match reco.sorted.first() {
        Some(n) if p.sort_metric == "cycle" => format!("{} (cycle)", clean_path(&n.id)),
        Some(n) => match num(n, &p.sort_metric) {
            Some(v) if v != 0.0 => format!(
                "{} ({} {})",
                clean_path(&n.id),
                attr_short(level, &p.sort_metric),
                fmt_val(v)
            ),
            _ => clean_path(&n.id),
        },
        None => "—".to_string(),
    }
}

/// Render the per-principle table (header + one line per row) into `out`.
fn render_principle_table(out: &mut String, rows: &[Row], want_warning: bool, want_info: bool) {
    let id_w = rows.iter().map(|r| r.id.len()).max().unwrap_or(6).max(6);
    let name_w = rows
        .iter()
        .map(|r| r.name.len())
        .max()
        .unwrap_or(9)
        .clamp(9, 34);
    let clip = |s: &str, w: usize| -> String {
        if s.len() > w {
            format!("{}…", &s[..w.saturating_sub(1)])
        } else {
            s.to_string()
        }
    };
    let mut header = format!("{:<id_w$}  {:<name_w$}", "PRESET", "PRINCIPLE");
    if want_warning {
        header.push_str("  WARN");
    }
    if want_info {
        header.push_str("  INFO");
    }
    header.push_str("  TOP MODULE");
    out.push_str(&header);
    out.push('\n');
    for r in rows {
        let mut line = format!("{:<id_w$}  {:<name_w$}", r.id, clip(&r.name, name_w));
        if want_warning {
            line.push_str(&format!("  {:>4}", r.warn));
        }
        if want_info {
            line.push_str(&format!("  {:>4}", r.info));
        }
        line.push_str(&format!("  {}", r.top));
        out.push_str(&line);
        out.push('\n');
    }
}

