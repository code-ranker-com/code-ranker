# code-ranker

[![CI](https://github.com/ffedoroff/code-ranker/actions/workflows/ci.yml/badge.svg)](https://github.com/ffedoroff/code-ranker/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/ffedoroff/code-ranker/branch/main/graph/badge.svg)](https://codecov.io/gh/ffedoroff/code-ranker)
[![code-ranker](https://img.shields.io/endpoint?url=https://api.code-ranker.com/badge/ffedoroff/cr-smoke-test.json)](https://reports.code-ranker.com/r/ffedoroff/cr-smoke-test/latest)
[![Crates.io](https://img.shields.io/crates/v/code-ranker.svg)](https://crates.io/crates/code-ranker)
[![npm](https://img.shields.io/npm/v/code-ranker.svg)](https://www.npmjs.com/package/code-ranker)
[![PyPI](https://img.shields.io/pypi/v/code-ranker.svg)](https://pypi.org/project/code-ranker/)
[![License](https://img.shields.io/crates/l/code-ranker.svg)](./LICENSE)

[![Website](https://img.shields.io/badge/website-code--ranker.com-1abc9c)](https://code-ranker.com)
[![Install the GitHub App](https://img.shields.io/badge/GitHub%20App-install-2c3e50?logo=github&logoColor=white)](https://github.com/apps/code-ranker-app/installations/new)

Structural-analysis tool for **Rust** (production-ready) plus **Python, TypeScript/JavaScript, Go, C, C++, C# and Markdown** (beta) codebases. Built **AI-agent-friendly first** — finds where a project has structural problems and hands an actionable shortlist to a human or an AI agent for the actual refactor.

**👉 Map your codebase's worst structural problems in 30 seconds — [jump to the Rust quick start](#rust-quick-start) and run it on your repo now.**

**Status:** 4.0.0 — the Rust analyzer is production-ready; the other languages are beta, so their output shapes may still change.

## Rust quick start

```sh
cargo install code-ranker  # install the CLI
code-ranker report .       # make html report in .code-ranker/ folder
```

`report .` needs no flags: it writes a self-contained HTML report (plus a JSON
snapshot) into `.code-ranker/`. Open the latest `…-<commit>.html` to explore the
dependency graph, per-file metrics, and the AI prompt generator. Everything
below is detail.

## Offline & private

code-ranker always runs **entirely on your machine**. It makes **no network calls**, sends **no telemetry or analytics**, and **never uploads your code or analysis results** anywhere. Generated HTML reports are self-contained — no CDN, no external requests, no tracking.

## AI agents friendly

**Hand your codebase to an AI agent and let it fix the worst spot.** code-ranker is built to feed work straight to an AI coding agent (Claude Code, Cursor, …). Run **`code-ranker docs ai`** in your repo — it prints a short, offline playbook (no network) that teaches the agent which two metrics matter (dependency cycles `ADP`, coupling `HK`) and the exact fix loop (scorecard → snapshot → fix → re-check → before/after report), tailored to your project's language.

Then just ask, e.g.:

- *"Run `code-ranker docs ai` and follow it: find the worst dependency cycle in this project and propose a refactor that breaks it — show me the plan before changing code."*
- *"Run `code-ranker docs ai` for the playbook, then find the most complex / highest-HK file and analyze how to split it; explain what the split buys for me (lower coupling, smaller blast radius). Take a **before report**, apply the split, take an **after report**, and show me the **HTML diff**."*

The agent drives the CLI itself — `code-ranker docs ai` spells out the commands and the loop, so no glue is needed. (Prefer a file in context? The same playbook lives at [docs/ai-skill.md](docs/ai-skill.md).)

## What it finds

- **Files that grew too complex and should be split.** Per-file cyclomatic / cognitive / Halstead / MI metrics; flags files above your threshold.
- **Strong coupling between files.** Computes fan-in / fan-out / HK on the file dependency graph; surfaces the files that everything depends on (or that depend on everything). Third-party libraries are tracked separately as depth-1 external nodes (`fan_out_external`), so they never inflate your internal-coupling numbers.
- **Cyclic dependencies.** Detects SCCs in the file graph — including the silent ones the compiler does not catch.
- **Files that are just too big.** Raw LOC, public surface size per file.

The tool **does not refactor for you**. It produces a structured, machine-readable list of problem spots and an offline HTML report a human or an LLM can act on.

## CI integration

Runs as a linter. Configure thresholds in `code-ranker.toml`; the CLI exits non-zero when the codebase breaches them — so a PR that introduces a new cycle, a file above your cognitive budget, or a file above your LOC limit fails the build.

```sh
code-ranker check . \
  --threshold file.cognitive=25 --threshold file.loc=800
```

The linter is the `check` command — exits non-zero on any cycle or threshold violation, e.g. a PR that introduces a new file-level cycle or a file above your LOC limit (`mutual` and `chain` cycle checks are on by default). See [docs/CLI.md](docs/code-ranker-cli/CLI.md) for all flags.

**Add it to your pipeline today** — one `code-ranker check` step stops new cycles and bloat from ever landing.

Prefer zero config? **[Install the GitHub App](https://github.com/apps/code-ranker-app/installations/new)** — it publishes a per-PR HTML structural report on every pull request, no workflow YAML to write. More at **[code-ranker.com](https://code-ranker.com)**.

## Full CLI

Written in Rust — fast, memory-safe, single static-ish binary with **no runtime dependencies** (no Python, no Node, no JVM, no shared libs to install). One file on PATH, done.

Two commands: `check` (linter — exits non-zero on violations; with `--baseline`, a relative regression gate) and `report` (snapshot JSON + offline HTML; with `--baseline`, a baseline↔current diff). Both accept a directory **or** an existing `.json`/`.html` snapshot as input — analyze once, then run cheap passes over the snapshot. No daemon, no language server, no plugin host required at runtime. Full reference: [docs/CLI.md](docs/code-ranker-cli/CLI.md).

## HTML report with dynamic diagrams

`code-ranker report` writes a single self-contained HTML file with:

- An interactive file dependency graph; third-party libraries appear as depth-1 external nodes in a distinct amber colour with dashed edges.
- Dagre-laid-out graph with pan/zoom and live filtering.
- Sortable table per metric; click a node to open its neighbourhood.
- "Prompt generator" panel that copies a ready-to-paste prompt (one for each principle: ADP, SRP, OCP, LSP, ISP, DIP, DRY, KISS, LoD, MISU, CoI, YAGNI; plus *Reduce Complexity*, *Split Components*) — feed the prompt + the selected nodes to your AI agent.

No network, no analytics, no telemetry. Open in any browser, share as a file.

**Live demo — code-ranker run on its own repo:** [interactive HTML report](https://ffedoroff.github.io/code-ranker/) · [JSON snapshot](https://ffedoroff.github.io/code-ranker/report.json) (regenerated on every push to `main`).

## Install

Pick any channel — all ship the same `code-ranker` binary (Linux, macOS, Windows). **Full guide with exact commands: [docs/installation.md](docs/installation.md).**

- **Shell / PowerShell installer** — prebuilt binary on PATH (universal)
- **Cargo** — `cargo install code-ranker` · [crates.io](https://crates.io/crates/code-ranker)
- **npm** — `npm install -g code-ranker` · [npm](https://www.npmjs.com/package/code-ranker)
- **pip / uv / pipx** — `pip install code-ranker` · [PyPI](https://pypi.org/project/code-ranker/)
- **Docker** — [Docker Hub](https://hub.docker.com/r/fedoroff/code-ranker) · [GHCR](https://github.com/ffedoroff/code-ranker/pkgs/container/code-ranker)

## Quick start

```sh
# lint a project — non-zero exit on violations (CI linter)
code-ranker check ./path/to/project

# analyze and write a snapshot JSON + offline HTML report
code-ranker report
# → .code-ranker/{ts}-{git-hash-3}.json + .code-ranker/{ts}-{git-hash-3}.html
#   (override paths via --output.<fmt>.path or [output.<fmt>] in code-ranker.toml)

# before / after refactor comparison: an HTML diff against a baseline snapshot
code-ranker report . --baseline .code-ranker/before.json
```

Built-in plugins for all nine supported languages (`rust` uses cargo + syn; Rust is production-ready, the rest are beta) — all compiled into the single binary, nothing to install.

## Documentation

- [Installation](docs/installation.md) — every install channel with exact commands
- [CLI](docs/code-ranker-cli/CLI.md) — commands, flags, and examples
- [Rule reference](docs/code-ranker-cli/ERRORS.md) — rule ids grouped by concern (`CYC`/`CPX`/`CPL`/`SIZ`), per-file thresholds (`file`), what each flags, and how to fix it
- [Config](docs/code-ranker-cli/config.md) — `code-ranker.toml` schema
- [AI agent skill](docs/ai-skill.md) — a short playbook to attach to an AI agent's context (the ADP/HK fix loop)
- [PRD](docs/PRD.md) — product requirements
- [DESIGN](docs/DESIGN.md) — technical design
- [Why structure matters](docs/why-structure-matters.md) — the empirical evidence (studies, books, statistics) behind the signals code-ranker measures
- [Principles corpus](languages/) — Rust / Python / TypeScript principle catalogues used by the prompt generator

## Try it now

```sh
cargo install code-ranker && code-ranker report . && open .code-ranker/
```

One command on any Rust project — you'll have an interactive structural map and an AI-ready shortlist in seconds. ⭐ the repo if it helps.

## License

Apache-2.0.
