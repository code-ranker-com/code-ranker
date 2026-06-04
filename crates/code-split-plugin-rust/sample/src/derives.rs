//! A crate referenced **only through a qualified derive**, with no `use`.
//!
//! `#[derive(serde::Serialize)]` names `serde` by a fully-qualified path inside
//! the derive list. Derive arguments are an opaque token stream, so this used to
//! produce no edge; the analyzer now parses qualified derive paths, so this file
//! gets an edge `derives.rs → serde` even though it never `use`s serde.

#[derive(serde::Serialize)]
pub struct OnlyDerived {
    pub v: i32,
}
