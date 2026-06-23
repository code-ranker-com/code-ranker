# code-ranker — AI agent skill

**TL;DR**: A short playbook for an AI assistant driving `code-ranker`, plus a
catalog of every principle and metric it checks. Each catalog entry is a
one-paragraph summary; run `code-ranker report --doc <ID>` to print any entry in
full (offline, straight to the terminal).

## Two commands

- **`check`** — a gate. Exits non-zero on a violation, writes no files.
- **`report`** — produces artifacts: a JSON snapshot, an HTML viewer, and the
  advisory **`scorecard`** (console triage) / **`prompt`** (LLM fix-prompt). Always
  exits `0`.

`[input]` is polymorphic: a directory is analyzed; a `.json` snapshot is read back
with no re-analysis. Keep old `.code-ranker/` snapshots — they are baselines for a
before/after diff (`--baseline <snapshot>`).

## The two that matter most

Fix one thing at a time, worst-first. Cycles (**ADP**) are structural — clear them
first; then coupling (**HK**). Focus on one metric or principle with `--focus` and
inspect the worst tier with `--severity warning`.

- **ADP** — dependency cycles; the module graph should be acyclic.
- **HK** — Henry–Kafura coupling, `HK = sloc × (fan_in × fan_out)²`: a large module
  on a busy crossroads of incoming/outgoing dependencies.

## The fix loop

```sh
code-ranker check .                                                   # the gate verdict
code-ranker report . --output.scorecard --focus cycle --top 1   # focus one metric/principle
code-ranker report . --output.prompt.path=stdout --top 1             # fix-prompt, worst module
```

`--focus` takes any catalog id below (a principle like `ADP`, or a metric like
`hk` / `cycle`): focusing on a metric frames the output by that metric; on a
principle, by that design principle.

## Principles & metrics

<!-- doc:tldr-index -->
