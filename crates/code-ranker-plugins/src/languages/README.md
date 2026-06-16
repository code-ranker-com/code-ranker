# `code-ranker-plugins` — adding / structuring a language

This crate hosts **all** language plugins. Each language is a self-contained
subfolder under `src/languages/`; everything language-specific that *can* be data
lives in that language's TOML config, and the per-language Rust is kept thin.
Follow the conventions below when adding a new language so its layout matches the
others.

## Layout

```
src/
  lib.rs               ← declares `languages` + re-exports the Plugin structs at the crate root
  config.rs            ← the config loader (defaults.toml merge + preset resolution)
  defaults.toml        ← the COMMON base config every language inherits
  engine/              ← the GENERIC tree-sitter metric engine (shared by all langs)
    mod.rs             ← compute / compute_functions driver + measure()
    core.rs            ← the Dialect trait + shared state types & node helpers (leaf)
    roles.rs           ← role-keyed node-kind tables (RoleCfg → resolved Roles)
    {structural,cognitive,halstead,loc}.rs ← the four shared sub-walks
  languages/           ← all language plugins live here, plus this guide
    README.md          ← this file (the adding-a-language guide)
    mod.rs             ← declares each language submodule (rust, python, …)
    <lang>/            ← one folder per language (rust, python, javascript, typescript, …)
      mod.rs           ← the LanguagePlugin impl — THIN (wires; see below)
      dialect.rs       ← the language's engine::Dialect (grammar + the few diffs)
      config.toml      ← language config: inherits defaults.toml, overrides only the diffs
      <structure>.rs … ← imperative-only code (dependency-graph builder)
      tests/
        <source-stem>.rs ← tests for the same-named source file (one file per source)
        sample/        ← the e2e fixture project + committed goldens
    ecmascript/        ← SHARED Dialect + structure builder for js + ts (they both reuse it)
```

The complexity metrics for **every** language run through one engine
(`src/engine/`), parameterized by a per-language `Dialect`
(`languages/<lang>/dialect.rs`)
and a role-keyed node-kind config (the `[roles]` / `[halstead]` / `[loc]`
sections of `config.toml`). A `Dialect` injects only the genuinely-divergent
predicates (Halstead operator context exceptions, exit rules, the cognitive
state-machine extras, closure/function classification, the LOC special-cases);
everything else is shared.

`javascript` and `typescript` are thin: they inject their grammar (+ a couple of
flags) and reuse `ecmascript/`. Do the same for any future dialect pair.

> **Terminology.** tree-sitter produces a *concrete* syntax tree (CST) — every
> token (punctuation, keywords, operators) is a node — not a classic AST. This
> guide and the codebase say "AST" loosely; the distinction that matters is
> *syntax nodes, not text*, which holds for the CST too.

## Conventions

### 1. Folders

- One folder per language under `src/languages/`. `src/languages/` holds the
  language subfolders (plus this README) — no `engine/`, `config.rs` or
  `defaults.toml`, which live at the crate `src/` root. The crate root keeps `lib.rs` (module list +
  re-exports), the shared `config.rs` / `defaults.toml`, and the generic
  `engine/`; none of those are language-specific.
- Logic shared by two related languages goes in its own shared module (e.g.
  `languages/ecmascript/`), which the language folders reuse — do not copy it.

### 2. `mod.rs` is thin

Keep `<lang>/mod.rs` to the wiring only:

- the `pub struct <Lang>Plugin;` and its `impl LanguagePlugin`
  (`code_ranker_plugin_api::LanguagePlugin`),
- which mostly **loads the config** (`config.toml` merged over `defaults.toml`)
  and **re-imports / calls** the imperative submodules.

Imperative code that genuinely cannot be data lives in dedicated submodules, not
inline in `mod.rs`: the metric walk is the shared `engine/` (a language adds only
its thin `dialect.rs`); the dependency-graph (structure) builder lives in its own
submodule(s) — Rust's `module_graph/…`, `collapse.rs`, …; Python's `structure.rs`;
js/ts reuse the shared `languages/ecmascript/`. Prefer `use`/re-export over
duplicating code.

**Default to a submodule — don't add code to `mod.rs` without need.** `mod.rs`
should hold the `impl LanguagePlugin` wiring and nothing else. A new free
function, helper, type, `const`, or any non-trivial logic goes in a named
submodule (`structure.rs`, `dialect.rs`, a new `*.rs`), not inline in `mod.rs`.
If `mod.rs` grows much past the trait impl, that is a smell — move the logic out.

### 3. Config: `config.toml` inherits `defaults.toml`

The language-agnostic **metric catalog** (the metrics themselves, their CEL
formulas and display specs) is NOT here — it is in
`crates/code-ranker-graph/metrics/builtin.toml`. A language never redefines a
metric; it only supplies/overrides the language-specific pieces.

`defaults.toml` holds the common baseline. Each `config.toml` **inherits it and
overrides only what differs**. A language config carries:

- **node-kind tables** — which tree-sitter `kind` strings fill each engine ROLE
  (operators/operands, branches, exits, statements, comments, spaces, …). The
  walk logic stays in `engine/`; *which kinds it counts* is data. (Reference: the
  `[roles]` / `[halstead]` / `[loc]` sections of `languages/rust/config.toml`.)
- **metric presets** — the recommendation lenses (e.g. `HK`), including the long
  prompt text (use TOML multiline strings).
- **spec overrides** — language-specific tweaks to a metric's display (e.g. the
  Rust `tloc`/`sloc` description mentioning `#[cfg(test)]`).
- **thresholds** — language-calibrated `info`/`warning` limits per metric.

Only spell out keys that differ from `defaults.toml`; inherited keys are dropped.

**Every parsing-affecting string constant goes in the config — never in Rust.**
If the engine keys on a string — and *especially* on a **list** of node-kind
strings — it must live in `config.toml`, not as a `const`/string literal in
`*.rs`. The walk logic stays in Rust; the strings it matches are data. A new
hardcoded `&["…", "…"]` of kinds in a source file is a bug — move it to the config.

**Identity roles need no config entry — don't copy-paste them.** A `[roles.one]`
singleton whose role name already IS its *named* grammar kind is redundant: the
resolver resolves the role name directly. So `if_statement = { kind =
"if_statement" }` is pure boilerplate — drop it. Spell out a `[roles.one]` entry
ONLY for an anonymous token or an alias — when the kind differs from the key or
it is not a named node — e.g. `kw_else = { kind = "else", named = false }`.

**The language config is free-form.** The loader accepts arbitrary fields, so a
language may introduce new sections/keys without touching the loader — only the
code that consumes a field needs to know about it. Prefer adding a config field
over adding a Rust constant.

**Audit every `"…"` string literal.** When adding or reviewing a language, open
each NON-test `.rs` file of that language and scan for double-quoted string
literals (`"foo"`, `"bar"`). For every one, ask in order:

1. Is it a **parsing/vocabulary** constant — a tree-sitter node kind, an edge
   kind, a node kind, a marker/keyword the code keys on? If yes, it is data:
   move it to a TOML config (it must NOT be a Rust `const`/literal).
2. If it must move, is it **language-neutral / shared** across languages (e.g. an
   edge kind like `uses`/`contains`, a node kind like `file`/`external`, their
   labels/descriptions)? If yes, put it in `defaults.toml` so every language
   inherits it — only language-specific values go in the per-language
   `config.toml`.

A **list** of extensions, a file-resolution order, project-detect marker
filenames (`package.json`, `Cargo.toml`, …), and skip-dir names are DATA → they
go in `config.toml`, not as Rust `const`/literals. The **test-path conventions**
the `is_test_path` predicate keys on are DATA too — the dir names / filename
infixes / stem suffixes / exact files / prefixes / suffixes
(`test_dirs` / `test_infixes` / `test_stem_suffixes` / `test_files` /
`test_prefixes` / `test_suffixes`), the **source-root** subfolders (`source_dirs`),
the **module-path strip extensions** (`module_strip_exts`), and the implicit
**index file** stem (`index_file`) all live in `config.toml`. Only the predicate
LOGIC (split on `/`, `contains`, `starts_with`, `ends_with`, first-component
checks) stays in Rust. Only these stay in Rust, because they bind a string to
something Rust-only, name a config entry, or are pure syntax: the `ext → grammar`
mapping (`match ext { "tsx" => tree_sitter_typescript::LANGUAGE_TSX, … }`, which
selects a grammar *type*); `config::*` **lookup keys** (the `get("key")` argument
that names a config section/entry — e.g. `string_table(cfg, "structure")`,
`edge_kind_id(cfg, "uses")`, `attr_key(cfg, "loc")` — validated against the
published table, never invented); id-structure punctuation in `format!` (`::` /
`#` / `@`); single-char / empty **syntax rules** (the leading-`.` skip rule,
Python's `_`/`__` dunder convention, the module separators `.` / `/`, an empty
path); and error/log/format strings.

**Everything else is data — including the former exceptions.** tree-sitter
field-name API navigation now lives in `[fields]`
(`child_by_field_name(&FIELDS.name)`); `syn` attribute idents (`test`/`cfg`/…) in
`[syn]`; Rust path keywords (`crate`/`self`/`super`) in `[path_keywords]`;
node-id namespace prefixes (`ext:`/`crate:`/`mod:`) in `[ids]`; visibility output
strings in `[visibility]`. These are plain `get(key)` **lookup tables** (no
auto-resolver), so unlike `[roles.one]` they DO carry identity entries
(`import_statement = "import_statement"`) by design — the vocabulary lives in
TOML, the Rust side only looks it up.

### 4. Tests: one file per source, in `tests/`, named after the source

**All tests live in the `tests/` folder.** A source file must never keep an
inline `#[cfg(test)] mod … { … }` test body — it is moved out to `tests/` and
pulled back in by a one-line `#[path]` module (below).

- Every source file's `#[cfg(test)]` tests live in `tests/<source-stem>.rs` —
  the test file is named **after the source file that wires it**:
  `dialect.rs` → `tests/dialect.rs`; `structure.rs` → `tests/structure.rs`;
  `module_graph/walk.rs` → `tests/walk.rs`. Do NOT name a test file after a
  metric/concept (`metrics_tests.rs`) or a source that no longer exists — rename
  it to match its current wiring source.
- One source wires **at most one** test file (named after it); don't split a
  source's tests across several files, and don't point two sources at one file.
- The source file pulls them in with a path-wired module, NOT an inline block:

  ```rust
  #[cfg(test)]
  #[path = "tests/dialect.rs"]   // for a nested source: "../tests/walk.rs"
  mod dialect_tests;
  ```

- The `tests/` folder has **no `mod.rs`** — files are included via `#[path]` from
  their source, so there is no `mod tests;` aggregator. A `mod.rs` source file's
  tests go to `tests/mod_rs.rs` (never `tests/mod.rs`, which is a special name).
- The e2e fixture project and its committed goldens
  (`code-ranker-report.json`, `code-ranker-check.sarif`,
  `code-ranker-check.codequality.json`, `code-ranker.toml`, fixture sources) live
  in `tests/sample/`.

### 5. Register the plugin

Add the struct to the CLI registry in
`crates/code-ranker-cli/src/plugin/mod.rs` → `registry()`:

```rust
Box::new(code_ranker_plugins::languages::<lang>::<Lang>Plugin),
```

The e2e harness (`crates/code-ranker-cli/tests/e2e.rs`, `sample_dir`) finds each
language's golden under `src/languages/<lang>/tests/sample/`.

## Where a new metric goes — defaults first

**Define metrics in the common config, not per language.** The shared metric
catalog — `crates/code-ranker-graph/metrics/builtin.toml`, the "defaults" every
language inherits — is where a metric's definition (its category, formula and
display spec) belongs. When you add a metric:

- If it is **not** single-language-specific, add it to the common defaults
  **first**. Aim to keep *all* metrics defined commonly there — a metric does not
  need to be re-declared per language.
- A commonly-defined metric that a given language's engine cannot produce is
  simply **omitted** for that language: with no value it is just not written to
  the JSON / HTML / SARIF output. So there is no harm in a language inheriting a
  metric it never emits — do not gate the catalog on what one language supports.
- Only put a metric in a `config.toml` when it is genuinely specific to that one
  language (and even then, prefer a common definition + a language override of
  just the differing pieces).

## Checklist — adding a language `foo`

1. `src/languages/foo/mod.rs` — `pub struct FooPlugin;` + thin `impl LanguagePlugin`.
2. `src/languages/foo/config.toml` — inherit `defaults.toml`; add node-kind tables,
   presets, thresholds, spec overrides (only the diffs).
3. Imperative submodules — the AST walk / structure builder (or reuse a shared
   module like `languages/ecmascript/`).
4. `src/languages/foo/tests/<source>.rs` for each source file;
   `src/languages/foo/tests/sample/` fixture + goldens.
5. `src/languages/mod.rs` — `pub mod foo;`; `lib.rs` — re-export `FooPlugin`.
6. CLI `registry()` — add `Box::new(code_ranker_plugins::languages::foo::FooPlugin)`.
7. Add the grammar dependency to this crate's `Cargo.toml`.

## Verify

```sh
cargo test -p code-ranker-plugins        # unit tests
cargo test -p code-ranker --test e2e     # goldens (32 cases)
make all                                 # build + test + clippy + lint + coverage
```

Regenerate a language's goldens after an intentional analyzer change by running
`code-ranker report` on its `tests/sample/` with that sample's `code-ranker.toml`
(see `docs/e2e.md` for the exact commands and header-freezing step).
