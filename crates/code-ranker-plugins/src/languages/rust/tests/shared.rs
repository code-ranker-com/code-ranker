use super::*;

#[test]
fn same_named_lib_and_bin_get_distinct_ids() {
    // A package with a lib `bat` and a bin `bat` must not share a module-id
    // namespace, or `crate::X` in the lib resolves to the bin's `main.rs`.
    assert_ne!(
        module_node_id("bat 1.0", "lib", "bat", &[]),
        module_node_id("bat 1.0", "bin", "bat", &[]),
    );
    assert_ne!(
        module_node_id("bat 1.0", "lib", "bat", &["theme".into()]),
        module_node_id("bat 1.0", "bin", "bat", &["theme".into()]),
    );
}
