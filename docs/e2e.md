# e2e fixtures & golden snapshots

Four tiny projects (one per language) exercise the `code-ranker` analyzer in the
**files** level of the generic graph model: nodes of `kind` `"file"` /
`"external"`, connected by `uses` (flow) and `reexports` / `contains` / `super`
(non-flow, structural) edges — the last being the Rust `use super::*` /
`use crate::<ancestor>::*` namespace pull.

Each fixture lives **next to its plugin crate** so the sample and the parser that
produces it sit together:

```
crates/code-ranker-plugin-rust/sample/
crates/code-ranker-plugin-python/sample/
crates/code-ranker-plugin-javascript/sample/
crates/code-ranker-plugin-typescript/sample/
```

Each project deliberately contains **both the dependency forms we DO detect and
the known blind spots**, documented in the source comments and pinned in its
`code-ranker-report.json`.

## How it works

- `crates/code-ranker-plugin-<lang>/sample/code-ranker.toml` — a self-contained
  config (plugin pinned, `ignore.tests = false` to override the **on-by-default**
  test skipping so test files stay in the graph and the fixture exercises them).
- `crates/code-ranker-plugin-<lang>/sample/code-ranker-report.json` — the **golden**
  JSON report (`schema_version: "2"`). The graph is already relativized to the
  `{target}` placeholder (machine-independent). The header (`generated_at`,
  `command`, `git`, versions, absolute paths, `timings`) is kept frozen /
  anonymized in the committed file, and normalized only at comparison time.
- `crates/code-ranker-cli/tests/e2e.rs` — the test: runs the binary on each
  sample, asserts the volatile header fields changed, normalizes them to a
  canonical value on both sides, and compares the whole structure
  **character-for-character** (100% match required).

```sh
cargo test -p code-ranker --test e2e    # verify against the committed goldens
```

## Regenerating the goldens

After an intentional analyzer change, regenerate each language's golden by
running `code-ranker report` on its sample with the sample's own config. Build the
binary first; the Rust sample resolves its crates from the warm cargo cache, so
analysis stays offline:

```sh
cargo build -p code-ranker
export CARGO_NET_OFFLINE=true
bin=target/debug/code-ranker

for lang in rust python javascript typescript; do
  dir="crates/code-ranker-plugin-$lang/sample"
  "$bin" report "$dir" \
    --config "$dir/code-ranker.toml" \
    --output.json.path="$dir/code-ranker-report.json"
done
```

The e2e test normalizes the volatile header (timestamp, command, git, versions,
absolute paths, per-stage `ms`) at comparison time, so the regenerated goldens
will pass as-is. To keep the **committed** file machine-independent and
churn-free, freeze that header — anonymize your home dir and zero the volatile
fields — before committing:

```sh
for lang in rust python javascript typescript; do
  f="crates/code-ranker-plugin-$lang/sample/code-ranker-report.json"
  python3 - "$f" "$PWD" "$HOME" <<'PY'
import sys, json
path, repo, home = sys.argv[1:4]
text = open(path).read().replace(repo, "/home/user/code-ranker").replace(home, "/home/user")
d = json.loads(text)
d["generated_at"] = "1970-01-01T00:00:00Z"
if "git" in d:
    d["git"] = {"branch": "main", "commit": "000000000000",
                "dirty_files": 0, "origin": "git@example.com:org/repo.git"}
for t in d.get("timings", []):
    t["ms"] = 0
open(path, "w").write(json.dumps(d, indent=2, sort_keys=True, ensure_ascii=False) + "\n")
PY
done
```

## Coverage matrix

Every project contains a file-to-file dependency cycle (`a ⇄ b`), an external
dependency, and a test file.

### Rust (`crates/code-ranker-plugin-rust/sample/`)

Detected: `use crate::`, groups `{}`, glob `*`, `as` rename, `super::`, inline
modules, `pub use` → `Reexports` edge, external crate via `use serde::` →
`External` node, and **bare qualified paths** in expressions/types with no
`use` — both cross-crate (`once_cell::sync::Lazy` → the crate's `External` node)
and intra-crate (`foo::run()` → a `Uses` edge `lib.rs → foo.rs`). A
`std::`/`core::` path is recognized but is NOT emitted as an External node.

**Namespace pull → `super` edge** (`src/foo/bar.rs`): a glob `use super::*`
that reaches *up* the module tree is emitted as the non-flow `super` kind
(`foo/bar.rs → foo.rs`), not `uses` — kept in the JSON but excluded from
fan-in / fan-out / HK / cycles; on the map drawn dashed on a leaf-node hover
(like `contains` / `reexports`).
Contrast `b.rs`'s `use super::a::alpha`: a *named* import of a sibling item is a
real `Uses` edge — only the glob pull from an ancestor becomes `super`.

**Cycle semantics** (`src/cycle_examples/`): a dedicated module spelling out which
edge forms close a cycle and which do not — a `reexports` + back-`uses` pair
(`reex_hub` / `reex_spoke`), a `super` glob where the child really uses a parent
item (`sup_parent` — a genuine but deprioritized cycle), and one where it does not
(`sup_loose` — benign scope-sugar). None are cycles today (only `uses` is flow);
the full reasoning is in [what-is-cycle.md](../principles/rust/what-is-cycle.md).

**Inline tests excluded from metrics** (`lib.rs`, `c.rs`, `derives.rs` carry
`#[cfg(test)] mod tests`): the complexity pass strips test items first, so those
lines are excluded from `sloc` / `lloc` / `cloc` / `blank` (and HK) and counted
as `tloc` instead — production metrics only. The test bodies reference items by
their own `crate::<mod>::…` path, so they add no cross-file edges.

**Cross-crate, submodule-precise** (the `helper` workspace member): a
`use helper::widget::{Widget, make}` resolves through `helper`'s library module
index to the **owning submodule file** — `cross.rs → helper/src/widget.rs` and
`→ helper/src/gadget.rs`, not a single edge to `helper`'s crate root. A path
that stops at a crate-root item (`use helper::TOP`) has no deeper submodule to
match and falls back to the root (`→ helper/src/lib.rs`). Registry crates with
no local library index still collapse to one `External` node.

**Qualified derive macros** (`derives.rs`): `#[derive(serde::Serialize)]` names
a crate by a fully-qualified path *inside* the derive list. Derive arguments are
an opaque token stream, but the analyzer parses qualified derive paths, so this
yields `derives.rs → serde` even with no `use serde` in the file. (A bare
single-segment derive like `#[derive(Serialize)]` still relies on the `use` for
its edge.)

**`#[path = "..."]` modules** (`relocated/custom.rs`): a module whose backing
file is at a non-default location is resolved via its `#[path]` attribute
(relative to the declaring file's directory), walked, and its edges captured
(`custom.rs → c.rs`). Without `#[path]` support the file and its edges would be
silently dropped.

Each `mod foo;` becomes a `File` node and emits a `Contains` edge
(parent → child). `Contains` is kept in the JSON snapshot as structural
ownership, but is **not** drawn on the main map and **not** counted in
fan_in / HK / cycles (directory grouping shows ownership instead).

Not detected: `extern crate serde;` (old syntax, no edge); a `use` **inside a
macro body** (the `use crate::c::gamma` hidden in the `pull_in_c!` body is
invisible, so `b.rs` gets no edge to `c.rs`); macro invocations (`make_answer!`,
`pull_in_c!`) — no nodes or edges. `macros.rs` is the remaining blind spot: it
is reached only via `mod macros;` (a `Contains`, excluded from fan_in), so it
has no information-flow inbound edge. Integration tests under `tests/` are a
separate target kind that is not analyzed at all.

### Python (`crates/code-ranker-plugin-python/sample/`)

Detected: `import`, dotted (`import os.path`), `as`, `from … import`, relative
(`from .`, `from .c`), grouped, star `*`, and — importantly — an **import inside a
function** (`base64`).

Not detected: dynamic/string-based imports — `importlib.import_module("…")`,
`__import__("…")`, `eval("…")` (the `xml`/`csv`/`hashlib` modules are absent).

### JavaScript (`crates/code-ranker-plugin-javascript/sample/`)

Detected: `import` (named/namespace/default/side-effect), `export … from`
(re-export), `require()` both local and external, extension and `index.*`
resolution.

Not detected: dynamic `import("./dynamic.js")` (`dynamic.js` is an orphan);
`require(variable)` with a computed argument.

### TypeScript (`crates/code-ranker-plugin-typescript/sample/`)

Detected: import without extension, `import type` (deduped with the value import
into a single edge), the `@/` alias → source root, `export * from`, external
`axios`, scoped `@scope/util`.

Not detected: dynamic `import("./lazy")` (`lazy.ts` is an orphan); a tsconfig
alias other than `@/` — `~utils/*` is **misclassified** as an external package
`~utils` instead of an edge to `util.ts`.
