use super::*;

#[test]
fn ecmascript_level_has_expected_structure() {
    // Merge an empty overlay onto `defaults.toml` so the level inherits the
    // shared `[edge_kinds.uses]` (both JS and TS get it the same way).
    let cfg = crate::config::load("");
    let level = ecmascript_level("files", &cfg);
    assert_eq!(level.name, "files");
    assert!(level.edge_kinds.contains_key("uses"));
    let uses = &level.edge_kinds["uses"];
    assert!(uses.flow);
    assert!(level.node_attributes.contains_key("loc"));
    assert!(level.node_attributes.contains_key("visibility"));
    assert!(level.node_attributes.contains_key("external"));
    assert!(level.edge_attributes.is_empty());
    assert!(level.attribute_groups.is_empty());
}
