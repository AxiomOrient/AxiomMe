use crate::run_startup_recovery;

use super::harness::TestHarness;

#[test]
fn startup_recovery_prunes_missing_index_entries() {
    let harness = TestHarness::setup();
    let stale_uri = "axiom://resources/web-editor/stale.md";
    harness
        .state
        .app
        .state
        .upsert_index_state(stale_uri, "deadbeef", 0, "indexed")
        .expect("insert stale index state");

    let before = harness
        .state
        .app
        .state
        .list_index_state_uris()
        .expect("list before recovery");
    assert!(before.iter().any(|x| x == stale_uri));

    let report = run_startup_recovery(&harness.state.app).expect("startup recovery");
    assert!(report.drift_count >= 1);
    assert_eq!(report.status, "success");
    assert_eq!(report.reindexed_scopes, 4);

    let after = harness
        .state
        .app
        .state
        .list_index_state_uris()
        .expect("list after recovery");
    assert!(!after.iter().any(|x| x == stale_uri));
}
