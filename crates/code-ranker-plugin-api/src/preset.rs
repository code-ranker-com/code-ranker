//! The Prompt-Generator [`Preset`] DTO.
//!
//! A `Preset` is **prompt-generator domain data**, not part of the parser
//! contract: a plugin *produces* its set (via
//! [`LanguagePlugin::presets`](crate::plugin::LanguagePlugin::presets)), but every
//! other consumer — the report snapshot, the `recommend` console/prompt views —
//! only *reads* presets and never parses anything. The type therefore lives here,
//! away from [`plugin`](crate::plugin), so those reporting consumers do not couple
//! to the parsing contract just to name this struct.

use serde::{Deserialize, Serialize};

/// A Prompt-Generator preset (a refactoring principle): a ready-to-paste AI
/// instruction plus how the UI seeds the node selection for it. Each plugin
/// builds its own set from config via [`LanguagePlugin::presets`](crate::plugin::LanguagePlugin::presets)
/// (the common catalog plus any language-specific presets).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    /// Stable id / short code shown on the button (e.g. `"ADP"`).
    pub id: String,
    /// Button label (usually the id).
    pub label: String,
    /// Full principle title (first heading of the generated prompt).
    pub title: String,
    /// The prompt body (Markdown, language-neutral by default).
    pub prompt: String,
    /// Link to the full principle doc, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_url: Option<String>,
    /// The metric the recommended-node list sorts by (an attribute key, or the
    /// pseudo-metric `"cycle"`).
    pub sort_metric: String,
    /// Which connection sets the preset pre-selects: any of `"in"`/`"out"`/`"common"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<String>,
}
