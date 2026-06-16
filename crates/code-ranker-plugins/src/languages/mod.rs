//! The language plugins.
//!
//! Each language lives in its own submodule (`rust`, `python`, `javascript`,
//! `typescript`); the JavaScript and TypeScript plugins share the
//! grammar-agnostic engine in [`ecmascript`]. The four plugin structs are
//! re-exported at the crate root via `lib.rs`.

pub mod ecmascript;
pub mod javascript;
pub mod python;
pub mod rust;
pub mod typescript;
