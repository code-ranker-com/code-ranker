# code-ranker — AI agent skill

**TL;DR**: `code-ranker` is a multi-language **structural analysis platform** an AI
assistant can drive. It builds a project's dependency graph, finds the structural
problems that make code hard to change — dependency **cycles** (ADP), heavy
**coupling** (Henry–Kafura), and complexity hotspots — ranks them worst-first, and
scores them against design principles (SOLID, DRY, KISS, …). It gates CI on your
thresholds, renders a self-contained HTML viewer of the graph, and emits
ready-to-use **AI fix-prompts**. One binary; a language plugin (Rust, Python,
JavaScript / TypeScript, Go, C / C++, C#, Markdown) is selected per project.

This is the short guide for driving it — the commands below operate the tool.

## Commands

- **`check [input]`** — the **gate**. Evaluates cycle rules and metric thresholds
  (with `--baseline`, only regressions), prints diagnostics, and **exits non-zero**
  on a violation. Writes no files — the CI entry point.
- **`report [input]`** — produces **artifacts**: a JSON snapshot, a self-contained
  HTML viewer, and the advisory **`scorecard`** (console triage) / **`prompt`** (an
  LLM fix-prompt). Always exits `0` — the analysis + refactoring entry point.
- **`ai`** — print this playbook. With a language plugin resolved it appends the
  full principle/metric catalog; with none it explains how to select one. No
  analysis; always exits `0`.
- **`help`** — usage for the binary or any command (`code-ranker --help`,
  `code-ranker <command> --help`, or `-h <command>`). Lists every flag.

`[input]` (default `.`) is polymorphic: a directory is analyzed; a `.json` / `.html`
snapshot is read back with no re-analysis. Keep old `.code-ranker/` snapshots — they
are baselines for a before/after diff (`--baseline <snapshot>`).

<!-- ai:select-start -->
## Select a language

`code-ranker` analyzes **one** language per run, selected by a plugin — and none
could be resolved here:

> {reason}

Pick one of: **{plugins}**. Either name it per run (applies to `check` / `report`
too):

```sh
code-ranker check . --plugin <name>
```

…or set it once in a `code-ranker.toml` at the project root, so every command picks
it up:

```toml
version = "{config_version}"
plugin = "<name>"
```

Then re-run `code-ranker ai` for the full playbook and the principle/metric catalog.
<!-- ai:select-end -->

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

Each entry summarizes one principle or metric; run `code-ranker report --doc <ID>`
to print its full doc (offline, straight to the terminal).

<!-- doc:tldr-index -->
