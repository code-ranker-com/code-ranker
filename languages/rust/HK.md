<!-- doc:base "What it measures" -->
<!-- doc:base "Why it matters" -->

## In Rust

Fan-in and fan-out are counted over real code dependencies (`use` paths,
qualified paths, derives) — the flow edges, not structural `mod` / `pub use`
re-export relationships. **One thing inflates `fan_in` artificially:** a
`pub(in <ancestor>)` restricted-visibility path is recorded as a fan-in edge up
to that ancestor even when nothing there `use`s the item (the same modelling as
[ADP](ADP.md)). A Rust module scores high HK when it is both widely imported and
imports widely:

- A `lib.rs` or `mod.rs` facade that re-exports and also orchestrates.
- A `types.rs` / `model.rs` that every layer imports *and* that itself pulls
  in serialization, validation, and persistence concerns.
- A `utils.rs` junk drawer that accumulates helpers used everywhere.

### Diagnose first: who imports this hub, and for what?

**Before choosing any remedy, run the _audiences_ check** — for each fan-in edge,
look at what that consumer actually imports from the hub. The remedy is decided by
the *shape* of that answer, **not** by the hub's own internal structure. Skipping
this step is the most common way to "fix" HK and barely move it.

`HK = sloc × (fan_in × fan_out)²` — the coupling term is squared, so the goal is
always to cut **how many edges reach the hub, or what they reach for** — never to
chase `sloc`.

- **Many consumers reach in for the SAME one or two items** (a shared type/alias, a
  constant, a trait) → those items live in the **wrong home**. **Move them to a new
  leaf module** and repoint the consumers. `fan_in` collapses (the edges now land on
  a leaf with no fan-out of its own), the hub keeps only what it genuinely uses, and
  nothing is split. On a real hub this is usually the **biggest, cheapest win** — and
  it is exactly the one a size-based split misses.
- **The in-edge is only a `pub(in <ancestor>)` visibility path**, not a real `use` →
  **narrow the visibility** (`pub(super)` if only the parent uses the item,
  `pub(crate)` if a sibling subtree does). The phantom edge dissolves; one-line change.
- **Consumers genuinely need DIFFERENT parts of the hub** (group A imports one cluster
  of items, group B another) → **split the hub by responsibility**, one module per
  role, so each consumer depends only on the part it uses. Both `fan_in` and `fan_out`
  drop for real, and each piece becomes independently testable.

### The trap: splitting the hub by its own internal seams

Carving a hub into sub-files along its *internal* structure — one file per trait
`impl`, a type-decl moved away from its `impl`, a `worker`/`runtime` helper — **shaves
`sloc` without cutting coupling**, and often *raises* `fan_in` (the new sub-files now
import the parent). The HK number drops a little; the hub stays the worst module. That
is metric-gaming, not decoupling. **A split is a real HK fix only when it changes _who
depends on what_** — i.e. when it follows the audiences check above, not the hub's own
table of contents. If the audiences check shows the coupling is a shared item in the
wrong home, **move that item out; do not carve up the hub.** (See "When a hub is
legitimate" below before splitting a genuine orchestrator.)
<!-- doc:base "Reducing it" -->

## When a hub is legitimate (accept, don't game)

Not every high-HK file should be split. A few are *irreducible by design* —
their coupling **is** the architecture, not an accident:

- **A core contract / trait** that every implementor depends on. Its `fan_in`
  grows with each implementation by definition, and it references the types its
  own signatures use (`fan_out`). The number is the cost of having one contract
  instead of many ad-hoc ones.
- **A top-level orchestrator** that wires every subsystem together. High
  `fan_out` is its whole job; pushing those dependencies elsewhere only moves
  the crossroads, it does not remove it.

Before accepting one, *prove* it is irreducible — apply the Step-4 test: would a
split **dissolve** coupling or merely **relocate** it? If every candidate
extraction either leaves `fan_in × fan_out` unchanged (you only shaved `sloc`)
or *raises* `fan_out` (you moved out a type the file's own signatures mention),
the hub is load-bearing and splitting it is metric-gaming, not decoupling.

When that holds, **accept it explicitly**: raise the `hk` threshold to sit just
above the hub, and record *why* right next to the value in config — name the
file, its role, and the factor that makes it irreducible. That turns a silent
suppression into a reviewed, documented decision, and keeps the gate meaningful
for the *next* file that crosses the line.

This is the exception, not the default. Raise the ceiling only for a NEW,
genuine hub you have proven irreducible; for everything else, prefer the split.
<!-- doc:base "How code-ranker surfaces it" -->

<!-- doc:base "A workflow: dissecting and splitting a high-HK file" -->

<!-- doc:base "Related principles" -->
<!-- doc:base "References" -->
