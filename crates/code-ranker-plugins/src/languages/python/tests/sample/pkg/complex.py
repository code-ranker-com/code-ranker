"""Module `complex` — exercises the per-function complexity metrics
rust-code-analysis computes for Python (cyclomatic, cognitive, exits) with real
values so the golden guards them. `args` / `closures` are NOT emitted for Python
by the analyzer, so the multi-arg function and the lambda below stay uncounted —
an analyzer scope limit, not a fixture gap. Dependency-free: no import edges."""


def classify(a, b, c):
    if a > 0:
        if b > 0:
            return a + b  # nested if -> cognitive; return -> exits
    elif a < 0 or c == 0:
        return c
    scale = lambda x: x * 2  # lambda present, but Python closures are not counted
    return scale(a) + b - c
