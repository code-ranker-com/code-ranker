# Rust plugin — research notes & TODO

Findings from exploring the Rust plugin's detection machinery and the optional
per-function analysis level. Reference for future work; not yet acted on.

## 1. How `unsafe` detection works today

The Rust structure builder parses each file with `syn::parse_file` and walks the
AST with `syn::visit::Visit` visitors. All of this lives under
`crates/code-ranker-plugins/src/languages/rust/`.

`unsafe` is counted by `UnsafeCounter` (`module_graph/visitors.rs:58-103`), which
catches:

- `unsafe { }` expression blocks (`visit_expr_unsafe`)
- `unsafe fn` — free functions, impl methods, trait methods
- `unsafe impl` and `unsafe trait`

It is purely syntactic: `unsafe` produced inside a macro body is invisible
(macros are never expanded), and the count is not type-checked.

The value flows through:

1. **Counted** — `walk.rs:57,64` drives `UnsafeCounter` over non-test items.
2. **Stored on the node** — `walk.rs:81` `node.unsafe_count = Some(...)`; field
   declared in `internal.rs:69`.
3. **Emitted as an attribute** — `collapse.rs:127-130` writes the `unsafe` key
   (omitted when zero, like other metrics).
4. **Declared / described** — `config.toml:312-318` (`[node_attributes.unsafe]`:
   label, description, `remediation`, `direction = "lower_better"`).
5. **Surfaced in the report** — `config.toml:367-371` adds the `unsafe` column and
   a project-wide stat.

## 2. Two extension paths for detecting more patterns

**A. New numeric counter (mirror `unsafe`)** — for anything that must be counted
over the AST:

- add a visitor (or a method on an existing one) in `module_graph/visitors.rs`;
- add the field to `internal.rs`;
- populate it in `module_graph/walk.rs`;
- emit it in `collapse.rs`;
- declare `[node_attributes.X]` (+ optionally `[report]`) in `config.toml`.

**B. CEL rule over already-collected facts — no Rust change.** `FactsCollector`
(`module_graph/visitors.rs:109-172`) already gathers string facts per file:
`derives`, `macros`, `attrs`, `imports`, `types`, `traits`. A
`[rules.checks.<id>]` CEL predicate (`contains`, `matches`, `startsWith`, …) can
match on these directly in TOML — see `crates/code-ranker-graph/src/checks.rs`.

## 3. Candidate anti-patterns → which path

Most "anti-patterns" are *expression calls* nobody counts yet (only `unsafe` is
counted; only `derives`/`macros`/`attrs`/`imports`/`types`/`traits` are collected
as facts):

| Pattern | Path | Hook |
| --- | --- | --- |
| `.unwrap()` / `.expect()` | A | `visit_expr_method_call` |
| `.clone()` | A | `visit_expr_method_call` |
| `panic!` / `todo!` / `unimplemented!` | **B** (already captured as macros) | — |
| `static mut` | A | `visit_item_static` (`mutability`) |
| `std::mem::transmute`, raw-pointer cast | A | `visit_expr_call` / `visit_expr_cast` |

Anti-pattern references gathered while researching:

- rust-unofficial/patterns — anti-patterns catalogue: <https://rust-unofficial.github.io/patterns/anti_patterns/index.html>
- The Rust Book, Unsafe Rust: <https://doc.rust-lang.org/book/ch20-01-unsafe-rust.html>

Common ones worth detecting: excessive `.clone()`, `.unwrap()`/`.expect()`,
`#![deny(warnings)]`, stringly-typed / `Box<dyn Error>` errors, `Deref`
polymorphism, blocking I/O in async, `static mut`, raw-pointer misuse,
`transmute`, panicking across FFI, Rust types in `#[repr(C)]`.

## 4. Per-function analysis — the optional `functions` level

Function-level analysis exists and works; it is **opt-in**, off by default.

**Enable** (config only — no CLI flag):

```toml
[levels]
functions = true
```

Default is `functions = false` (`crates/code-ranker-cli/src/config/defaults.toml:68-69`).

**What it emits** — alongside `graphs.files`, the JSON report gains
`graphs.functions` with one node per function/method/closure. Each node has:

- `kind` from the language vocabulary (`function` / `method` / …),
- `name`, `parent` (the file id), and an id of the form `<file>#<name>@<line>`,
- all per-file metrics computed over the function body (`loc/sloc/cloc`,
  `cyclomatic`, `cognitive`, Halstead, …).

Verified by `functions_level_is_opt_in` (`crates/code-ranker-cli/tests/e2e.rs:1358`):
with `functions = true`, function `f` gets `cyclomatic = 2`, method `m` gets
`kind = "method"`.

**Wiring:**

- Level declared in `languages/rust/mod.rs:96-106` (`Level { name: "functions" }`).
- Nodes built in `function_units()` (`languages/rust/mod.rs:149-172`): the file is
  re-read, inline tests stripped, `dialect::compute_functions()` splits it into
  functions and computes metrics.
- Orchestrator gates the level on `cfg.levels.functions`
  (`crates/code-ranker-cli/src/pipeline.rs:113-138, 284-303`).
- Implemented for all languages (Rust, Python, Go, C#, C, ECMAScript/TS, Markdown).

**Limitations (deliberate):**

- No edges — the `functions` level has an empty `edge_kinds`, so there is no call
  graph (calls between functions are not tracked).
- No coupling metrics — `fan_in`/`fan_out`, HK index, and cycles are file-level only.
- Tests (`#[cfg(test)]` / `#[test]`) are excluded, as in the per-file metrics.
