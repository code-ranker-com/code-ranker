use super::*;

/// `compute_functions` finds top-level fns, impl methods, and counts a nested
/// closure on its owning fn (covers collect_functions / unit_for / fn_kind).
#[test]
fn compute_functions_covers_fn_method_closure() {
    let src = b"fn f(x: i32) -> i32 { if x > 0 { return 1; } 0 }\n\
                struct S;\n\
                impl S { fn m(&self, y: i32) -> i32 { y } }\n\
                fn g() { let c = |z: i32| z + 1; let _ = c(1); }\n";
    let units = compute_functions(src);
    let names: Vec<&str> = units.iter().map(|u| u.name.as_str()).collect();
    assert!(names.contains(&"f"), "fn f: {names:?}");
    assert!(names.contains(&"m"), "method m: {names:?}");
    assert!(names.contains(&"g"), "fn g: {names:?}");

    let f = units.iter().find(|u| u.name == "f").unwrap();
    assert_eq!(f.kind, "fn");
    assert!(f.inputs.branches >= 1.0, "f has an `if` branch");
    assert!(
        f.inputs.exits >= 1.0,
        "f has a `return` / value-returning exit"
    );

    let m = units.iter().find(|u| u.name == "m").unwrap();
    assert_eq!(m.kind, "method");

    let g = units.iter().find(|u| u.name == "g").unwrap();
    assert!(g.inputs.closures >= 1.0, "g defines a closure");
}

#[test]
fn compute_functions_empty_on_no_functions() {
    assert!(compute_functions(b"const X: i32 = 1;\n").is_empty());
}
