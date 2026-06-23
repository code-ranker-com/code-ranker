use super::*;

#[test]
fn fill_select_injects_live_values_into_the_doc_template() {
    let reason = "ambiguous project in .: markers for multiple plugins found (rust, markdown) — pass --plugin to choose";
    let md = fill_select(&templates::ai_doc_intro().unwrap(), reason);

    // Intro + command list (the prose authored in base/AI.md) is kept…
    assert!(
        md.contains("code-ranker — AI agent skill"),
        "intro head present"
    );
    assert!(
        md.contains("## Commands") && md.contains("**`help`**") && md.contains("**`report"),
        "command list present"
    );
    // …and the Select-a-language template is filled with live values.
    assert!(md.contains("## Select a language"), "setup section present");
    assert!(
        md.contains(reason),
        "{{reason}} replaced with the diagnostic"
    );
    assert!(
        md.contains(&plugin::names()),
        "{{plugins}} replaced with the registry names"
    );
    assert!(
        md.contains(&format!("version = \"{CONFIG_VERSION}\"")),
        "{{config_version}} replaced with the live CONFIG_VERSION"
    );

    // No placeholder leaks…
    for ph in ["{reason}", "{plugins}", "{config_version}"] {
        assert!(!md.contains(ph), "placeholder {ph} fully substituted");
    }
    // …and the catalog is withheld until a language is chosen.
    assert!(
        !md.contains("## Principles & metrics") && !md.contains("### ADP"),
        "catalog omitted: {md}"
    );
}
