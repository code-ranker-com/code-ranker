//! Code Ranker language plugins, merged into one crate.
//!
//! Each language lives in its own module under [`languages`] (`rust`, `python`,
//! `javascript`, `typescript`); the JavaScript and TypeScript plugins share the
//! grammar-agnostic engine in [`languages::ecmascript`]. The four plugin structs
//! are re-exported at the crate root (e.g. [`RustPlugin`]).

pub mod config;
pub mod engine;
pub mod languages;

pub use languages::javascript::JavascriptPlugin;
pub use languages::python::PythonPlugin;
pub use languages::rust::RustPlugin;
pub use languages::typescript::TypescriptPlugin;
