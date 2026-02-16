use super::*;

#[test]
fn build_scope_key_requires_identifier_by_scope() {
    assert_eq!(
        build_scope_key(OmScope::Session, Some("s1"), None, None).expect("session key"),
        "session:s1"
    );
    assert_eq!(
        build_scope_key(OmScope::Thread, None, Some("t1"), None).expect("thread key"),
        "thread:t1"
    );
    assert_eq!(
        build_scope_key(OmScope::Resource, None, None, Some("r1")).expect("resource key"),
        "resource:r1"
    );
    assert_eq!(
        build_scope_key(OmScope::Session, None, None, None).expect_err("must fail"),
        OmTransformError::MissingScopeIdentifier("session_id")
    );
}

#[test]
fn build_scope_key_trims_identifiers_and_ignores_unrelated_ids() {
    assert_eq!(
        build_scope_key(
            OmScope::Session,
            Some("  s-main  "),
            Some("t-ignored"),
            Some("r-ignored"),
        )
        .expect("session key"),
        "session:s-main"
    );
    assert_eq!(
        build_scope_key(
            OmScope::Thread,
            Some("s-ignored"),
            Some("  t-main  "),
            Some("r-ignored"),
        )
        .expect("thread key"),
        "thread:t-main"
    );
    assert_eq!(
        build_scope_key(
            OmScope::Resource,
            Some("s-ignored"),
            Some("t-ignored"),
            Some("  r-main  "),
        )
        .expect("resource key"),
        "resource:r-main"
    );
}

#[test]
fn build_scope_key_rejects_missing_identifier_for_each_scope() {
    assert_eq!(
        build_scope_key(OmScope::Session, Some("   "), Some("t1"), Some("r1"))
            .expect_err("must fail"),
        OmTransformError::MissingScopeIdentifier("session_id")
    );
    assert_eq!(
        build_scope_key(OmScope::Thread, Some("s1"), None, Some("r1")).expect_err("must fail"),
        OmTransformError::MissingScopeIdentifier("thread_id")
    );
    assert_eq!(
        build_scope_key(OmScope::Thread, Some("s1"), Some("   "), Some("r1"))
            .expect_err("must fail"),
        OmTransformError::MissingScopeIdentifier("thread_id")
    );
    assert_eq!(
        build_scope_key(OmScope::Resource, Some("s1"), Some("t1"), None).expect_err("must fail"),
        OmTransformError::MissingScopeIdentifier("resource_id")
    );
}
