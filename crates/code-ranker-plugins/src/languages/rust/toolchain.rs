//! Rust/Cargo toolchain probing: the path roots used to shorten external node
//! ids in the snapshot, and the `rustc` version string. These shell out to
//! `rustc` and read `$CARGO_HOME` / `$RUSTUP_HOME`, so they are Rust-specific and
//! live in the plugin rather than the language-agnostic orchestrator.

use code_ranker_plugin_api::log;

/// The Rust/Cargo toolchain path roots used to shorten external node ids in the
/// snapshot: `cargo` (`$CARGO_HOME`), `registry` (the crates.io source dir),
/// `rustup` (`$RUSTUP_HOME`), and `rust-src` (the stdlib source under the active
/// sysroot).
pub(super) fn rust_toolchain_roots() -> Vec<(String, String)> {
    let mut roots = Vec::new();
    let home = std::env::var("HOME").unwrap_or_default();

    let cargo = std::env::var("CARGO_HOME").unwrap_or_else(|_| format!("{home}/.cargo"));
    let rustup = std::env::var("RUSTUP_HOME").unwrap_or_else(|_| format!("{home}/.rustup"));

    if !cargo.is_empty() {
        // Auto-detect crates.io registry hash dir (e.g. index.crates.io-<hash>).
        let registry_src = format!("{cargo}/registry/src");
        if let Ok(entries) = std::fs::read_dir(&registry_src) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("index.crates.io") {
                    roots.push(("registry".to_string(), format!("{registry_src}/{name}")));
                    break;
                }
            }
        }
        roots.push(("cargo".to_string(), cargo));
    }
    if !rustup.is_empty() {
        // Add rust-src root: sysroot/lib/rustlib/src/rust/library — shortens stdlib
        // paths from {rustup}/toolchains/.../library/... to {rust-src}/...
        if which::which("rustc").is_ok()
            && let Ok(out) = log::timed("rustc --print sysroot", || {
                std::process::Command::new("rustc")
                    .args(["--print", "sysroot"])
                    .output()
            })
            && out.status.success()
        {
            let sysroot = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let rust_lib = format!("{sysroot}/lib/rustlib/src/rust/library");
            if std::path::Path::new(&rust_lib).exists() {
                roots.push(("rust-src".to_string(), rust_lib));
            }
        }
        roots.push(("rustup".to_string(), rustup));
    }
    roots
}

/// The `rustc` semantic version (the second whitespace-token of `rustc
/// --version`), or `None` when `rustc` is absent or the call fails.
pub(super) fn version_string() -> Option<String> {
    which::which("rustc").ok()?;
    let out = log::timed("rustc --version", || {
        std::process::Command::new("rustc")
            .arg("--version")
            .output()
    })
    .ok()?;
    if out.status.success() {
        Some(
            String::from_utf8_lossy(&out.stdout)
                .split_whitespace()
                .nth(1)
                .unwrap_or("unknown")
                .to_string(),
        )
    } else {
        None
    }
}
