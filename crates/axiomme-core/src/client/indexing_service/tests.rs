use std::time::Duration;

use tempfile::tempdir;

use super::*;
use crate::config::TierSynthesisMode;
#[cfg(unix)]
use std::os::unix::fs::symlink;

#[test]
fn resolve_tier_synthesis_mode_defaults_to_deterministic() {
    assert_eq!(
        resolve_tier_synthesis_mode(Some("semantic")),
        TierSynthesisMode::SemanticLite
    );
    assert_eq!(
        resolve_tier_synthesis_mode(Some("semantic-lite")),
        TierSynthesisMode::SemanticLite
    );
    assert_eq!(
        resolve_tier_synthesis_mode(Some("deterministic")),
        TierSynthesisMode::Deterministic
    );
    assert_eq!(
        resolve_tier_synthesis_mode(Some("")),
        TierSynthesisMode::Deterministic
    );
    assert_eq!(
        resolve_tier_synthesis_mode(None),
        TierSynthesisMode::Deterministic
    );
}

#[test]
fn resolve_internal_tier_policy_defaults_to_virtual() {
    assert_eq!(
        resolve_internal_tier_policy(Some("persist")),
        InternalTierPolicy::Persist
    );
    assert_eq!(
        resolve_internal_tier_policy(Some("virtual")),
        InternalTierPolicy::Virtual
    );
    assert_eq!(
        resolve_internal_tier_policy(Some("invalid-policy")),
        InternalTierPolicy::Virtual
    );
    assert_eq!(
        resolve_internal_tier_policy(Some("")),
        InternalTierPolicy::Virtual
    );
    assert_eq!(
        resolve_internal_tier_policy(None),
        InternalTierPolicy::Virtual
    );
}

#[test]
fn virtual_internal_tier_policy_skips_persist_for_internal_scopes() {
    assert!(should_persist_scope_tiers(
        Scope::Resources,
        InternalTierPolicy::Virtual
    ));
    assert!(should_persist_scope_tiers(
        Scope::User,
        InternalTierPolicy::Virtual
    ));
    assert!(!should_persist_scope_tiers(
        Scope::Temp,
        InternalTierPolicy::Virtual
    ));
    assert!(!should_persist_scope_tiers(
        Scope::Queue,
        InternalTierPolicy::Virtual
    ));
    assert!(should_persist_scope_tiers(
        Scope::Temp,
        InternalTierPolicy::Persist
    ));
    assert!(should_persist_scope_tiers(
        Scope::Queue,
        InternalTierPolicy::Persist
    ));
}

#[test]
fn initialize_honors_internal_tier_policy_for_internal_scopes() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init");

    let policy = app.config.indexing.internal_tier_policy;
    let queue_root = AxiomUri::root(Scope::Queue);
    let temp_root = AxiomUri::root(Scope::Temp);
    let internal_should_persist = matches!(policy, InternalTierPolicy::Persist);
    assert_eq!(
        app.fs.abstract_path(&queue_root).exists(),
        internal_should_persist
    );
    assert_eq!(
        app.fs.overview_path(&queue_root).exists(),
        internal_should_persist
    );
    assert_eq!(
        app.fs.abstract_path(&temp_root).exists(),
        internal_should_persist
    );
    assert_eq!(
        app.fs.overview_path(&temp_root).exists(),
        internal_should_persist
    );

    let resources_root = AxiomUri::root(Scope::Resources);
    assert!(app.fs.abstract_path(&resources_root).exists());
    assert!(app.fs.overview_path(&resources_root).exists());
}

#[test]
fn virtual_internal_policy_prunes_existing_generated_tiers_in_internal_scopes() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    if matches!(
        app.config.indexing.internal_tier_policy,
        InternalTierPolicy::Persist
    ) {
        return;
    }
    app.fs.initialize().expect("fs init");

    let queue_uri = AxiomUri::parse("axiom://queue/traces").expect("queue uri");
    app.fs
        .create_dir_all(&queue_uri, true)
        .expect("mkdir queue");
    fs::write(app.fs.resolve_uri(&queue_uri).join(".abstract.md"), "stale").expect("write");
    fs::write(app.fs.resolve_uri(&queue_uri).join(".overview.md"), "stale").expect("write");

    let temp_uri = AxiomUri::parse("axiom://temp/ingest").expect("temp uri");
    app.fs.create_dir_all(&temp_uri, true).expect("mkdir temp");
    fs::write(app.fs.resolve_uri(&temp_uri).join(".abstract.md"), "stale").expect("write");
    fs::write(app.fs.resolve_uri(&temp_uri).join(".overview.md"), "stale").expect("write");

    app.ensure_scope_tiers().expect("ensure scope tiers");

    assert!(!app.fs.resolve_uri(&queue_uri).join(".abstract.md").exists());
    assert!(!app.fs.resolve_uri(&queue_uri).join(".overview.md").exists());
    assert!(!app.fs.resolve_uri(&temp_uri).join(".abstract.md").exists());
    assert!(!app.fs.resolve_uri(&temp_uri).join(".overview.md").exists());
}

#[test]
fn semantic_tier_synthesis_emits_summary_and_topics() {
    let temp = tempdir().expect("tempdir");
    let dir = temp.path().join("semantic-tier");
    fs::create_dir_all(&dir).expect("mkdir");
    fs::write(
        dir.join("auth.md"),
        "OAuth authorization flow with token exchange",
    )
    .expect("write auth");
    fs::write(
        dir.join("storage.md"),
        "SQLite persistence cache storage guide",
    )
    .expect("write storage");

    let uri = AxiomUri::parse("axiom://resources/semantic-tier").expect("uri parse");
    let (abstract_text, overview) =
        synthesize_directory_tiers(&uri, &dir, TierSynthesisMode::SemanticLite)
            .expect("synthesize semantic");

    assert!(abstract_text.contains("semantic summary"));
    assert!(overview.contains("Summary:"));
    assert!(overview.contains("- topics:"));
    assert!(overview.contains("- auth.md"));
    assert!(overview.contains("- storage.md"));
}

#[test]
fn semantic_tier_synthesis_falls_back_for_empty_directory() {
    let temp = tempdir().expect("tempdir");
    let dir = temp.path().join("empty-tier");
    fs::create_dir_all(&dir).expect("mkdir");

    let uri = AxiomUri::parse("axiom://resources/empty-tier").expect("uri parse");
    let (abstract_text, overview) =
        synthesize_directory_tiers(&uri, &dir, TierSynthesisMode::SemanticLite)
            .expect("synthesize empty");

    assert_eq!(
        abstract_text,
        "axiom://resources/empty-tier contains 0 items"
    );
    assert!(overview.contains("(empty)"));
}

#[test]
fn synthesize_directory_tiers_ignores_generated_and_internal_files() {
    let temp = tempdir().expect("tempdir");
    let dir = temp.path().join("tier-visible");
    fs::create_dir_all(&dir).expect("mkdir");
    fs::write(dir.join(".abstract.md"), "generated").expect("write");
    fs::write(dir.join(".overview.md"), "generated").expect("write");
    fs::write(dir.join(".meta.json"), "{}").expect("write");
    fs::write(dir.join(".relations.json"), "[]").expect("write");
    fs::write(dir.join("messages.jsonl"), "{}\n").expect("write");
    fs::write(dir.join("visible.md"), "visible").expect("write");

    let uri = AxiomUri::parse("axiom://resources/tier-visible").expect("uri parse");
    let (abstract_text, overview) =
        synthesize_directory_tiers(&uri, &dir, TierSynthesisMode::Deterministic)
            .expect("synthesize deterministic");

    assert!(abstract_text.contains("contains 1 items"));
    assert!(overview.contains("visible.md"));
    assert!(!overview.contains(".abstract.md"));
    assert!(!overview.contains(".overview.md"));
    assert!(!overview.contains(".meta.json"));
    assert!(!overview.contains(".relations.json"));
    assert!(!overview.contains("messages.jsonl"));
}

#[test]
fn synthesize_directory_tiers_fails_for_non_directory_path() {
    let temp = tempdir().expect("tempdir");
    let file_path = temp.path().join("not-a-directory.txt");
    fs::write(&file_path, "payload").expect("write file");

    let uri = AxiomUri::parse("axiom://resources/not-a-directory").expect("uri parse");
    let err = synthesize_directory_tiers(&uri, &file_path, TierSynthesisMode::Deterministic)
        .expect_err("must fail");
    assert!(matches!(err, AxiomError::Io(_)));
}

#[test]
fn ensure_directory_tiers_rewrites_when_directory_contents_change() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.fs.initialize().expect("fs init");

    let uri = AxiomUri::parse("axiom://resources/tier-refresh").expect("uri parse");
    app.fs.create_dir_all(&uri, true).expect("mkdir");
    app.fs
        .write(&uri.join("alpha.md").expect("join"), "alpha payload", true)
        .expect("write alpha");

    app.ensure_directory_tiers(&uri).expect("first synth");
    let before = app.fs.read_overview(&uri).expect("before overview");

    app.fs
        .write(&uri.join("beta.md").expect("join"), "beta payload", true)
        .expect("write beta");
    app.ensure_directory_tiers(&uri).expect("second synth");
    let after = app.fs.read_overview(&uri).expect("after overview");

    assert_ne!(before, after);
    assert!(after.contains("beta.md"));
}

#[test]
fn reindex_uri_tree_truncates_large_text_files_for_indexing() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init");

    let uri = AxiomUri::parse("axiom://resources/large-index").expect("uri parse");
    app.fs.create_dir_all(&uri, true).expect("mkdir");
    let large = "x".repeat(MAX_INDEX_READ_BYTES + 128);
    fs::write(app.fs.resolve_uri(&uri).join("big.txt"), large).expect("write large");

    app.reindex_uri_tree(&uri).expect("reindex");

    let leaf_uri = "axiom://resources/large-index/big.txt";
    let index = app.index.read().expect("index read");
    let record = index.get(leaf_uri).expect("record");
    assert!(
        record.content.contains("[indexing truncated at"),
        "large payload should be truncated in indexed content"
    );
    drop(index);
}

#[test]
fn reindex_uri_tree_updates_index_state_when_only_mtime_changes() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init");

    let uri = AxiomUri::parse("axiom://resources/mtime-index").expect("uri parse");
    app.fs.create_dir_all(&uri, true).expect("mkdir");
    let file_path = app.fs.resolve_uri(&uri).join("same.txt");
    fs::write(&file_path, "same-content").expect("write v1");

    app.reindex_uri_tree(&uri).expect("first reindex");
    let state_v1 = app
        .state
        .get_index_state("axiom://resources/mtime-index/same.txt")
        .expect("state v1")
        .expect("missing v1");

    std::thread::sleep(Duration::from_millis(2));
    fs::write(&file_path, "same-content").expect("write v2");
    app.reindex_uri_tree(&uri).expect("second reindex");

    let state_v2 = app
        .state
        .get_index_state("axiom://resources/mtime-index/same.txt")
        .expect("state v2")
        .expect("missing v2");
    assert_eq!(state_v1.0, state_v2.0, "hash should stay stable");
    assert!(state_v2.1 >= state_v1.1, "mtime should refresh in state");
}

#[test]
fn directory_ancestor_chain_lists_parent_to_root_in_order() {
    let uri = AxiomUri::parse("axiom://resources/a/b/c.md").expect("uri parse");
    let parent = uri.parent().expect("parent");
    let chain = directory_ancestor_chain(&parent)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert_eq!(
        chain,
        vec![
            "axiom://resources/a/b".to_string(),
            "axiom://resources/a".to_string(),
            "axiom://resources".to_string(),
        ]
    );
}

#[cfg(unix)]
#[test]
fn reindex_document_with_ancestors_ignores_unrelated_invalid_names() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init");

    let root_uri = AxiomUri::parse("axiom://resources/targeted-reindex").expect("root parse");
    app.fs.create_dir_all(&root_uri, true).expect("mkdir");
    let doc_uri = root_uri.join("guide.md").expect("join");
    app.fs
        .write(&doc_uri, "# Guide\n\nalpha targeted token", true)
        .expect("write doc");

    let bad_path = app.fs.resolve_uri(&root_uri).join("bad\\name.md");
    fs::write(bad_path, "invalid sibling path").expect("write bad sibling");

    app.reindex_document_with_ancestors(&doc_uri)
        .expect("targeted reindex");

    let result = app
        .find(
            "alpha targeted token",
            Some("axiom://resources/targeted-reindex"),
            Some(5),
            None,
            None,
        )
        .expect("find");
    assert!(
        result
            .query_results
            .iter()
            .any(|hit| hit.uri == "axiom://resources/targeted-reindex/guide.md")
    );
}

#[cfg(unix)]
#[test]
fn reindex_uri_tree_skips_broken_symlink_entries() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init");

    let uri = AxiomUri::parse("axiom://resources/reindex-broken").expect("uri parse");
    app.fs.create_dir_all(&uri, true).expect("mkdir");

    let broken_target = temp.path().join("missing-target.txt");
    let broken_link = app.fs.resolve_uri(&uri).join("broken-link.md");
    symlink(&broken_target, &broken_link).expect("symlink");

    app.reindex_uri_tree(&uri).expect("reindex");
    let indexed = app.state.list_index_state_uris().expect("list index state");
    assert!(
        !indexed
            .iter()
            .any(|item| item == "axiom://resources/reindex-broken/broken-link.md")
    );
}

#[cfg(unix)]
#[test]
fn reindex_uri_tree_does_not_follow_symlinked_external_files() {
    let temp = tempdir().expect("tempdir");
    let outside = tempdir().expect("outside");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init");

    let uri = AxiomUri::parse("axiom://resources/reindex-symlink").expect("uri parse");
    app.fs.create_dir_all(&uri, true).expect("mkdir");

    let outside_file = outside.path().join("secret.md");
    fs::write(&outside_file, "SYMLINK_ESCAPE_SENTINEL").expect("write outside file");
    let inside_link = app.fs.resolve_uri(&uri).join("linked-secret.md");
    symlink(&outside_file, &inside_link).expect("symlink file");

    app.reindex_uri_tree(&uri).expect("reindex");
    let indexed = app.state.list_index_state_uris().expect("list index state");
    assert!(
        !indexed
            .iter()
            .any(|item| item == "axiom://resources/reindex-symlink/linked-secret.md")
    );

    let result = app
        .find(
            "SYMLINK_ESCAPE_SENTINEL",
            Some("axiom://resources/reindex-symlink"),
            Some(5),
            None,
            None,
        )
        .expect("find");
    assert!(!result.query_results.iter().any(|hit| {
        hit.uri == "axiom://resources/reindex-symlink/linked-secret.md"
            || hit.abstract_text.contains("SYMLINK_ESCAPE_SENTINEL")
    }));
}
