use super::*;

#[test]
fn visibility_as_str_covers_all_variants() {
    assert_eq!(Visibility::Public.as_str(), "public");
    assert_eq!(Visibility::Crate.as_str(), "crate");
    assert_eq!(Visibility::Super.as_str(), "super");
    assert_eq!(Visibility::Private.as_str(), "private");
    assert_eq!(
        Visibility::Restricted {
            path: "pub(in crate::x)".to_string()
        }
        .as_str(),
        "pub(in crate::x)"
    );
}
