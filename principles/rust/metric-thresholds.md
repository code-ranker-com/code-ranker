# Metric thresholds (Rust)

**TL;DR**: Per-metric two-tier thresholds used to colour the report's
metric counts. **`info`** (yellow) = "worth a look", **`warning`** (red) =
"likely a problem". They are **empirically calibrated and language-specific** —
the values below are for **Rust** and do not transfer to other languages.

These thresholds are advisory: in the HTML report they only drive the colour of
the per-metric count badge, never a hard pass/fail filter. They are *not* the
`check` linter's `rules.thresholds.file` limits (those are user-configured,
project-by-project).

## How the values were derived

Empirically calibrated on a corpus of **21 Rust crates** (each ≥ 2 000 SLOC,
drawn from a 35-repository internal corpus). For each metric the per-project
**maximum file value** was taken, then the thresholds were chosen so that,
measured **per project, binary** (a project "breaches" a tier if it has at
least one file over the line):

- **`info`** is breached by **~50 %** of projects, and
- **`warning`** is breached by **~10 %** of projects.

So a `warning`-level value marks a file that is unusual even among real-world
Rust code (top ~10 % of projects), while `info` marks something common enough
that about half of projects have at least one (≈ "above the median project").

## Rust thresholds

| Metric | `info` (≥) | `warning` (≥) | % projects breaching info / warning |
|---|---|---|---|
| `hk` (Henry–Kafura) | 150 000 | 10 000 000 | ~52 % / ~10 % |
| `sloc` (source lines) | 800 | 3 000 | ~48 % / ~10 % |
| `fan_out` | 8 | 18 | ~52 % / ~10 % |
| `item_count` | 20 | 50 | ~43 % / ~10 % |

`cyclomatic` and `cognitive` are now **aggregated across a file's functions** and
emitted (the file value is the sum over its functions; a function-less file — a
pure type/`clap` declaration — omits both rather than reporting a vacuous `1`).
They are **not yet threshold-calibrated for Rust**, though: no `info` / `warning`
line is set, so they surface as raw metrics and sort keys but raise no violations.
Calibrating a distribution against a Rust corpus is future work.

### Notes & caveats

- **`hk`** lives on a large scale (`hk = sloc × (fan_in × fan_out)²`), so the
  `warning` line is in the millions — only the very top of projects reach it.
- Calibrated on Rust **services/libraries**; very small crates (< 2 000 SLOC)
  were excluded so the percentiles reflect substantial codebases.

## Per language

These are **Rust** numbers. Other languages have different idioms (file size,
module granularity, coupling style) and therefore different distributions, so
each language gets its own calibrated block.
