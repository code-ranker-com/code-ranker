<!-- doc:base "What it measures" -->
<!-- doc:base "Why it matters" -->

## In Rust

Fan-in and fan-out are counted over real code dependencies (`use` paths,
qualified paths, derives) — the flow edges, not structural `mod`/`pub use`
relationships. A Rust module scores high HK when it is both widely imported
and imports widely:

- A `lib.rs` or `mod.rs` facade that re-exports and also orchestrates.
- A `types.rs` / `model.rs` that every layer imports *and* that itself pulls
  in serialization, validation, and persistence concerns.
- A `utils.rs` junk drawer that accumulates helpers used everywhere.
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
