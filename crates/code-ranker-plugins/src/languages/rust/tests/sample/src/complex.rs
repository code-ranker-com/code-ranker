//! Module `complex` — exists to exercise the per-function COMPLEXITY metrics so
//! the golden actually guards them. These (`cyclomatic`, `cognitive`, `exits`,
//! `args`, `closures`) and the structural `unsafe` count are summed over a file's
//! functions; a fixture of trivial stubs leaves them at their omit value and the
//! golden would assert nothing. So this file deliberately packs, across two
//! functions: nested branches (`cognitive`), early `return`s (`exits`), several
//! arguments (`args`), a closure (`closures`), and an `unsafe` block (`unsafe`).
//!
//! It is dependency-free on purpose — no `use` / qualified-path edges — so it
//! adds only a structural `Contains` edge from `lib.rs` and does not perturb the
//! graph cases the other fixtures pin.

// Nested branches + early returns + a closure: drives cyclomatic, cognitive,
// exits, args (3 fn params + the closure's 1) and closures.
pub fn classify(a: i32, b: i32, c: i32) -> i32 {
    if a > 0 {
        if b > 0 {
            return a + b; // early return → exits, nested if → cognitive
        }
    } else if a < 0 || c == 0 {
        return c;
    }
    let scale = |x: i32| x * 2; // closure → closures, its `x` → args
    scale(a) + b - c
}

// `unsafe` block in production code → the `unsafe` metric (the Rust plugin counts
// it; it is excluded only when gated behind `#[cfg(test)]`).
pub fn first_byte(p: *const u8, n: usize) -> usize {
    let head = unsafe { *p };
    n + head as usize
}
