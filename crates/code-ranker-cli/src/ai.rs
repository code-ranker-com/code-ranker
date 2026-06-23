//! The `ai` subcommand: print the offline AI-agent playbook to stdout.
//!
//! Unlike `check` / `report` it never analyzes — it only resolves which language
//! plugin applies (explicit `--plugin` > config `plugin` > auto-detect from the
//! `[input]` directory's markers) to choose the output:
//!   - **resolved** → the full embedded `base/AI.md` playbook + principle/metric
//!     catalog (the agent can analyze, so no plugin-setup noise);
//!   - **unresolved** (no marker, or ambiguous markers) → a brief product intro plus
//!     how to select a plugin, with the catalog withheld until a language is chosen.

use anyhow::Result;
use code_ranker_graph::version::CONFIG_VERSION;
use std::path::Path;

use crate::{config, plugin, templates};

pub(crate) fn run(input: &Path, plugin_arg: Option<&str>, config_entries: &[String]) -> Result<()> {
    // `ai` is a doc command: a missing or broken config must not fail it. Read the
    // config best-effort, only for its `plugin` key.
    let cfg_plugin = config::load(input, config_entries, &[], &[], &[])
        .ok()
        .and_then(|loaded| loaded.config.plugin);

    let md = match plugin::resolve_plugin(plugin_arg, cfg_plugin.as_deref(), input) {
        Ok(_) => templates::ai_doc()?,
        Err(reason) => fill_select(&templates::ai_doc_intro()?, &reason.to_string()),
    };

    print!("{}", templates::with_trailing_newline(md));
    Ok(())
}

/// Fill the *Select a language* template authored in `base/AI.md` (returned in the
/// intro by [`templates::ai_doc_intro`]) with the live values: the resolver
/// diagnostic (`reason` — no marker / ambiguous markers), the built-in plugin names,
/// and the config-schema version. The prose lives in the doc; only the values are
/// injected here.
fn fill_select(intro: &str, reason: &str) -> String {
    intro
        .replace("{reason}", reason)
        .replace("{plugins}", &plugin::names())
        .replace("{config_version}", CONFIG_VERSION)
}

#[cfg(test)]
#[path = "ai_test.rs"]
mod tests;
