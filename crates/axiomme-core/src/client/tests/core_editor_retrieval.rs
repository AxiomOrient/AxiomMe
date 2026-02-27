use super::*;
use crate::models::QueueEventStatus;

#[test]
fn end_to_end_add_and_find() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("input.txt");
    fs::write(&src, "OAuth flow with auth code.").expect("write input");

    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let result = app
        .find("oauth", Some("axiom://resources/demo"), Some(5), None, None)
        .expect("find failed");

    assert!(!result.query_results.is_empty());
    assert!(result.trace.is_some());
}

#[test]
fn mkdir_enforces_scope_policy_and_enqueues_reindex_event() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let err = app
        .mkdir("axiom://queue/mkdir-should-fail")
        .expect_err("mkdir under queue scope must fail");
    assert!(matches!(err, AxiomError::PermissionDenied(_)));

    let target = "axiom://resources/mkdir-contract/subdir";
    app.mkdir(target).expect("mkdir resources");
    let target_uri = AxiomUri::parse(target).expect("target uri parse");
    assert!(app.fs.exists(&target_uri));

    let queued = app
        .state
        .fetch_outbox(QueueEventStatus::New, 100)
        .expect("fetch new events");
    assert!(queued.iter().any(|event| {
        event.event_type == "reindex"
            && event.uri == target
            && event.payload_json.get("op").and_then(|v| v.as_str()) == Some("mkdir")
    }));
}

#[test]
fn tree_and_glob_reflect_resource_view_for_client_api() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    app.mkdir("axiom://resources/tree-glob/nested")
        .expect("mkdir nested");
    let guide = AxiomUri::parse("axiom://resources/tree-glob/guide.md").expect("guide uri parse");
    let readme =
        AxiomUri::parse("axiom://resources/tree-glob/nested/readme.txt").expect("readme uri");
    app.fs
        .write(&guide, "# Guide\n\nclient tree glob", true)
        .expect("write guide");
    app.fs
        .write(&readme, "nested text", true)
        .expect("write readme");

    let tree = app.tree("axiom://resources/tree-glob").expect("tree");
    assert_eq!(tree.root.uri, "axiom://resources/tree-glob");
    let child_uris = tree
        .root
        .children
        .iter()
        .map(|node| node.uri.as_str())
        .collect::<Vec<_>>();
    assert!(child_uris.contains(&"axiom://resources/tree-glob/guide.md"));
    assert!(child_uris.contains(&"axiom://resources/tree-glob/nested"));

    let glob = app
        .glob("**/*.md", Some("axiom://resources/tree-glob"))
        .expect("glob");
    assert!(
        glob.matches
            .iter()
            .any(|uri| uri == "axiom://resources/tree-glob/guide.md"),
        "glob should include markdown leaf created in target scope"
    );
}

#[test]
fn backend_status_exposes_embedding_profile() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let status = app.backend_status().expect("backend status");
    let profile = crate::embedding::embedding_profile();
    assert_eq!(status.retrieval_backend, "memory");
    assert_eq!(status.retrieval_backend_policy, "memory_only");
    assert_eq!(status.embedding.provider, profile.provider);
    assert_eq!(status.embedding.vector_version, profile.vector_version);
    assert_eq!(status.embedding.dim, profile.dim);
}

#[test]
fn find_result_serializes_contract_fields_for_abstract_and_query_plan() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("contract_fields_input.txt");
    fs::write(&src, "OAuth flow with authorization code grant.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/contract-fields"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let result = app
        .search(
            "oauth flow",
            Some("axiom://resources/contract-fields"),
            None,
            Some(5),
            None,
            None,
        )
        .expect("search failed");
    assert!(!result.query_results.is_empty());

    let encoded = serde_json::to_value(&result).expect("serialize");
    let first = encoded["query_results"][0]
        .as_object()
        .expect("query result object");
    assert!(first.contains_key("abstract"));
    assert!(!first.contains_key("abstract_text"));
    assert!(encoded["query_plan"]["typed_queries"].is_array());
}

#[test]
fn markdown_editor_load_save_updates_search_index() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("markdown_editor_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(
        corpus_dir.join("guide.md"),
        "# Guide\n\nalpha_token markdown editor baseline",
    )
    .expect("write md");

    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/markdown-editor"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let uri = "axiom://resources/markdown-editor/guide.md";
    let loaded = app.load_markdown(uri).expect("load markdown");
    assert!(loaded.content.contains("alpha_token"));
    assert!(!loaded.etag.is_empty());

    let saved = app
        .save_markdown(
            uri,
            "# Guide\n\nbeta_token markdown editor updated",
            Some(&loaded.etag),
        )
        .expect("save markdown");
    assert_eq!(saved.uri, uri);
    assert_eq!(saved.reindexed_root, "axiom://resources/markdown-editor");
    assert!(!saved.etag.is_empty());

    let reloaded = app.load_markdown(uri).expect("reload markdown");
    assert!(reloaded.content.contains("beta_token"));

    let found = app
        .find(
            "beta_token",
            Some("axiom://resources/markdown-editor"),
            Some(5),
            None,
            None,
        )
        .expect("find updated token");
    assert!(
        found
            .query_results
            .iter()
            .any(|x| x.uri == "axiom://resources/markdown-editor/guide.md")
    );
}

#[test]
fn markdown_editor_rejects_etag_conflict() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("markdown_etag_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(corpus_dir.join("guide.md"), "# Guide\n\netag_v1").expect("write md");
    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/markdown-etag"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let uri = "axiom://resources/markdown-etag/guide.md";
    let loaded = app.load_markdown(uri).expect("load");
    app.save_markdown(uri, "# Guide\n\netag_v2", Some(&loaded.etag))
        .expect("first save");

    let err = app
        .save_markdown(uri, "# Guide\n\netag_v3", Some(&loaded.etag))
        .expect_err("must conflict");
    assert!(matches!(err, AxiomError::Conflict(_)));
}

#[test]
fn markdown_editor_save_logs_latency_metrics() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("markdown_metrics_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(corpus_dir.join("guide.md"), "# Guide\n\nmetrics_v1").expect("write md");
    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/markdown-metrics"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let uri = "axiom://resources/markdown-metrics/guide.md";
    let loaded = app.load_markdown(uri).expect("load");
    app.save_markdown(uri, "# Guide\n\nmetrics_v2", Some(&loaded.etag))
        .expect("save");

    let logs = app
        .list_request_logs_filtered(20, Some("markdown.save"), Some("ok"))
        .expect("list logs");
    let entry = logs.first().expect("markdown.save log entry");
    let details = entry.details.as_ref().expect("details");
    assert!(
        details.get("save_ms").is_some(),
        "save_ms metric must be logged"
    );
    assert!(
        details.get("reindex_ms").is_some(),
        "reindex_ms metric must be logged"
    );
    assert!(
        details.get("total_ms").is_some(),
        "total_ms metric must be logged"
    );
}

#[test]
fn request_logs_skip_corrupted_jsonl_lines_when_valid_entries_exist() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let _ = app
        .find("oauth", Some("invalid://bad-target"), Some(5), None, None)
        .expect_err("find should fail");

    let request_log_uri = crate::catalog::request_log_uri().expect("request log uri");
    app.fs
        .append(&request_log_uri, "{invalid-json\n", true)
        .expect("append corrupt line");

    let logs = app.list_request_logs(20).expect("list logs");
    assert!(
        logs.iter()
            .any(|entry| entry.operation == "find" && entry.status == "error")
    );
}

#[test]
fn request_logs_fail_with_diagnostics_when_all_lines_are_invalid() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let request_log_uri = crate::catalog::request_log_uri().expect("request log uri");
    app.fs
        .write(&request_log_uri, "{invalid-json\n", true)
        .expect("write corrupt log");

    let err = app
        .list_request_logs(20)
        .expect_err("all-invalid request logs must fail");
    assert!(matches!(err, AxiomError::Validation(_)));
    assert!(err.to_string().contains("skipped 1 invalid lines"));
}

#[test]
fn move_rejects_cross_scope_transfer() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let source_dir = temp.path().join("cross_scope_move");
    fs::create_dir_all(&source_dir).expect("mkdir");
    fs::write(source_dir.join("guide.md"), "# Guide\n\ncross scope").expect("write md");
    app.add_resource(
        source_dir.to_str().expect("source str"),
        Some("axiom://resources/cross-scope"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let err = app
        .mv(
            "axiom://resources/cross-scope/guide.md",
            "axiom://user/cross-scope/guide.md",
        )
        .expect_err("cross-scope move must be rejected");
    assert!(matches!(err, AxiomError::PermissionDenied(_)));
}

#[test]
fn rm_prunes_index_state_prefix_entries() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("rm_prune_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(corpus_dir.join("guide.md"), "# Guide\n\nrm prune").expect("write md");
    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/rm-prune"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let before = app.state.list_index_state_uris().expect("list before");
    assert!(
        before
            .iter()
            .any(|uri| uri.starts_with("axiom://resources/rm-prune")),
        "expected indexed URIs under rm-prune before delete"
    );

    app.rm("axiom://resources/rm-prune", true).expect("rm");

    let after = app.state.list_index_state_uris().expect("list after");
    assert!(
        !after
            .iter()
            .any(|uri| uri.starts_with("axiom://resources/rm-prune")),
        "index_state entries under rm-prune must be removed"
    );
}

#[test]
fn mv_prunes_old_prefix_from_index_state_and_search_results() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("mv_prune_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(corpus_dir.join("guide.md"), "# Guide\n\nmoved_token").expect("write md");
    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/mv-src"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    app.mv("axiom://resources/mv-src", "axiom://resources/mv-dst")
        .expect("mv");

    let old_scope = app
        .find(
            "moved_token",
            Some("axiom://resources/mv-src"),
            Some(10),
            None,
            None,
        )
        .expect("find old scope");
    assert!(
        old_scope.query_results.is_empty(),
        "old prefix must not retain stale in-memory records"
    );

    let new_scope = app
        .find(
            "moved_token",
            Some("axiom://resources/mv-dst"),
            Some(10),
            None,
            None,
        )
        .expect("find new scope");
    assert!(!new_scope.query_results.is_empty());

    let uris = app.state.list_index_state_uris().expect("list index_state");
    assert!(
        !uris
            .iter()
            .any(|uri| uri.starts_with("axiom://resources/mv-src")),
        "old prefix index_state entries must be removed"
    );
    assert!(
        uris.iter()
            .any(|uri| uri.starts_with("axiom://resources/mv-dst")),
        "new prefix index_state entries must exist after reindex"
    );
}

#[test]
fn sessions_list_reads_updated_at_from_session_meta() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let session = app.session(Some("s-meta-updated-at"));
    session.load().expect("session load");
    session
        .add_message("user", "check updated_at fidelity")
        .expect("add message");

    let sessions = app.sessions().expect("sessions");
    let listed = sessions
        .iter()
        .find(|item| item.session_id == "s-meta-updated-at")
        .expect("session list item");

    let session_uri = AxiomUri::parse("axiom://session/s-meta-updated-at").expect("uri parse");
    let meta_path = app.fs.resolve_uri(&session_uri).join(".meta.json");
    let raw_meta = fs::read_to_string(meta_path).expect("read meta");
    let meta: crate::models::SessionMeta = serde_json::from_str(&raw_meta).expect("parse meta");

    assert_eq!(listed.updated_at, meta.updated_at);
}

#[test]
fn session_delete_removes_scope_tree_and_index_prefix() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let session_root = AxiomUri::parse("axiom://session/s-delete").expect("session root");
    app.fs
        .create_dir_all(&session_root, true)
        .expect("create session dir");
    app.fs
        .write(
            &session_root.join("notes.md").expect("join notes"),
            "delete_me_session_token",
            true,
        )
        .expect("write notes");

    app.reindex_all().expect("reindex all");

    let before = app.state.list_index_state_uris().expect("list before");
    assert!(
        before
            .iter()
            .any(|uri| uri.starts_with("axiom://session/s-delete")),
        "session prefix should be indexed before delete"
    );

    assert!(app.delete("s-delete").expect("delete should succeed"));
    assert!(!app.fs.exists(&session_root));

    let after = app.state.list_index_state_uris().expect("list after");
    assert!(
        !after
            .iter()
            .any(|uri| uri.starts_with("axiom://session/s-delete")),
        "session prefix index_state entries must be pruned after delete"
    );

    let docs = app.state.list_search_documents().expect("list docs after");
    assert!(
        !docs
            .iter()
            .any(|doc| doc.uri.starts_with("axiom://session/s-delete")),
        "session prefix search docs must be pruned after delete"
    );

    assert!(
        !app.delete("s-delete")
            .expect("second delete should be false"),
        "deleting a missing session should return false"
    );
}

#[test]
fn add_resource_replacing_target_prunes_stale_index_entries() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let first = temp.path().join("replace_target_first");
    fs::create_dir_all(&first).expect("mkdir first");
    fs::write(first.join("old.md"), "stale_token only in first version").expect("write old");
    app.add_resource(
        first.to_str().expect("first str"),
        Some("axiom://resources/replace-target"),
        None,
        None,
        true,
        None,
    )
    .expect("add first");

    let second = temp.path().join("replace_target_second");
    fs::create_dir_all(&second).expect("mkdir second");
    fs::write(second.join("new.md"), "fresh_token only in second version").expect("write new");
    app.add_resource(
        second.to_str().expect("second str"),
        Some("axiom://resources/replace-target"),
        None,
        None,
        true,
        None,
    )
    .expect("add second");

    let stale = app
        .find(
            "stale_token",
            Some("axiom://resources/replace-target"),
            Some(10),
            None,
            None,
        )
        .expect("find stale token");
    assert!(
        !stale
            .query_results
            .iter()
            .any(|hit| hit.uri == "axiom://resources/replace-target/old.md"),
        "old file must not remain in retrieval results after replacement"
    );

    let fresh = app
        .find(
            "fresh_token",
            Some("axiom://resources/replace-target"),
            Some(10),
            None,
            None,
        )
        .expect("find fresh token");
    assert!(!fresh.query_results.is_empty());

    let indexed = app.state.list_index_state_uris().expect("list index");
    assert!(
        !indexed
            .iter()
            .any(|uri| uri == "axiom://resources/replace-target/old.md"),
        "old file index state should be pruned"
    );
    assert!(
        indexed
            .iter()
            .any(|uri| uri == "axiom://resources/replace-target/new.md"),
        "new file should be indexed"
    );
}

#[test]
fn document_editor_json_load_save_updates_search_index() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("document_editor_json_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(
        corpus_dir.join("config.json"),
        "{\"feature\":\"alpha\",\"enabled\":true}",
    )
    .expect("write json");
    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/document-editor-json"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let uri = "axiom://resources/document-editor-json/config.json";
    let loaded = app.load_document(uri).expect("load document");
    assert!(loaded.content.contains("\"alpha\""));

    let saved = app
        .save_document(
            uri,
            "{\n  \"feature\": \"beta\",\n  \"enabled\": true\n}",
            Some(&loaded.etag),
        )
        .expect("save document");
    assert_eq!(
        saved.reindexed_root,
        "axiom://resources/document-editor-json"
    );

    let found = app
        .find(
            "beta",
            Some("axiom://resources/document-editor-json"),
            Some(5),
            None,
            None,
        )
        .expect("find updated token");
    assert!(found.query_results.iter().any(|x| x.uri == uri));
}

#[test]
fn document_editor_rejects_invalid_json() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("document_editor_invalid_json_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(corpus_dir.join("config.json"), "{\"feature\":\"alpha\"}").expect("write json");
    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/document-editor-invalid-json"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let uri = "axiom://resources/document-editor-invalid-json/config.json";
    let loaded = app.load_document(uri).expect("load");
    let err = app
        .save_document(uri, "{\"feature\":", Some(&loaded.etag))
        .expect_err("invalid json must fail");
    assert!(matches!(err, AxiomError::Validation(_)));
}

#[test]
fn document_editor_rejects_invalid_yaml() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("document_editor_invalid_yaml_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(
        corpus_dir.join("config.yaml"),
        "feature: alpha\nenabled: true\n",
    )
    .expect("write yaml");
    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/document-editor-invalid-yaml"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let uri = "axiom://resources/document-editor-invalid-yaml/config.yaml";
    let loaded = app.load_document(uri).expect("load");
    let err = app
        .save_document(uri, "feature: [", Some(&loaded.etag))
        .expect_err("invalid yaml must fail");
    assert!(matches!(err, AxiomError::Validation(_)));
}

#[test]
fn markdown_editor_rejects_non_markdown_internal_and_tier_targets() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("markdown_validation_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(corpus_dir.join("notes.txt"), "plain text").expect("write txt");
    fs::write(corpus_dir.join("guide.md"), "# Guide\n\nok").expect("write md");
    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/markdown-validation"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let txt_uri = "axiom://resources/markdown-validation/notes.txt";
    let txt_load = app.load_markdown(txt_uri).expect_err("must reject txt");
    assert!(matches!(txt_load, AxiomError::Validation(_)));

    let txt_save = app
        .save_markdown(txt_uri, "new content", None)
        .expect_err("must reject txt save");
    assert!(matches!(txt_save, AxiomError::Validation(_)));

    let queue_uri = AxiomUri::parse("axiom://queue/editor/test.md").expect("queue uri");
    app.fs
        .write(&queue_uri, "# queue", true)
        .expect("write queue file");
    let queue_load = app
        .load_markdown("axiom://queue/editor/test.md")
        .expect_err("must reject internal scope");
    assert!(matches!(queue_load, AxiomError::PermissionDenied(_)));

    let tier_uri = "axiom://resources/markdown-validation/.overview.md";
    let tier_load = app
        .load_markdown(tier_uri)
        .expect_err("must reject tier file");
    assert!(matches!(tier_load, AxiomError::PermissionDenied(_)));
}

#[cfg(unix)]
#[test]
fn markdown_editor_save_ignores_unrelated_invalid_sibling_path() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("markdown_rollback_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(corpus_dir.join("guide.md"), "# Guide\n\nrollback_old_token").expect("write md");
    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/markdown-rollback"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let root_uri = AxiomUri::parse("axiom://resources/markdown-rollback").expect("root parse");
    let bad_path = app.fs.resolve_uri(&root_uri).join("bad\\name.md");
    fs::write(bad_path, "this path forces reindex uri conversion failure").expect("write bad");

    let uri = "axiom://resources/markdown-rollback/guide.md";
    let loaded = app.load_markdown(uri).expect("load");

    app.save_markdown(uri, "# Guide\n\nrollback_new_token", Some(&loaded.etag))
        .expect("save should succeed");

    let after = app.load_markdown(uri).expect("load after save");
    assert!(!after.content.contains("rollback_old_token"));
    assert!(after.content.contains("rollback_new_token"));
}

#[test]
#[ignore = "manual performance evidence for targeted reindex vs full-tree reindex"]
fn markdown_reindex_targeted_vs_full_tree_p95_reference() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("reindex_perf_corpus");
    fs::create_dir_all(corpus_dir.join("nested/deeper")).expect("mkdir nested");

    for idx in 0..240 {
        let file = corpus_dir.join(format!("doc-{idx:03}.md"));
        fs::write(file, format!("# Doc {idx}\n\nseed token {idx}")).expect("write corpus file");
    }
    for idx in 0..120 {
        let file = corpus_dir
            .join("nested/deeper")
            .join(format!("deep-{idx:03}.md"));
        fs::write(file, format!("# Deep {idx}\n\nseed deep token {idx}"))
            .expect("write deep corpus file");
    }

    fs::write(corpus_dir.join("target.md"), "# Target\n\nseed target").expect("write target");

    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/reindex-perf"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let target_uri =
        AxiomUri::parse("axiom://resources/reindex-perf/target.md").expect("target parse");
    let parent_uri = target_uri.parent().expect("parent uri");

    let mut targeted_ms = Vec::<u128>::new();
    let mut full_tree_ms = Vec::<u128>::new();
    let iterations = 30usize;

    for idx in 0..iterations {
        let content = format!("# Target\n\nmode:targeted\niteration:{idx}");
        app.fs
            .write_atomic(&target_uri, &content, false)
            .expect("write target content");
        let started = std::time::Instant::now();
        app.reindex_document_with_ancestors(&target_uri)
            .expect("targeted reindex");
        targeted_ms.push(started.elapsed().as_millis());
    }

    for idx in 0..iterations {
        let content = format!("# Target\n\nmode:full_tree\niteration:{idx}");
        app.fs
            .write_atomic(&target_uri, &content, false)
            .expect("write target content");
        let started = std::time::Instant::now();
        app.reindex_uri_tree(&parent_uri)
            .expect("full-tree reindex");
        full_tree_ms.push(started.elapsed().as_millis());
    }

    let targeted_p95 = percentile_p95(&targeted_ms);
    let full_tree_p95 = percentile_p95(&full_tree_ms);
    println!(
        "reindex_perf_reference targeted_p95_ms={targeted_p95} full_tree_p95_ms={full_tree_p95}"
    );
}

#[test]
#[ignore = "manual performance evidence for markdown lock contention under unrelated writes"]
fn markdown_editor_load_latency_under_unrelated_save_pressure_reference() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("markdown_lock_perf_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir corpus");
    for idx in 0..300 {
        let file = corpus_dir.join(format!("seed-{idx:03}.md"));
        fs::write(
            file,
            format!("# Seed {idx}\n\nlock benchmark seed token {idx}"),
        )
        .expect("write seed file");
    }
    fs::write(corpus_dir.join("writer.md"), "# Writer\n\nseed writer").expect("write writer");
    fs::write(corpus_dir.join("reader.md"), "# Reader\n\nseed reader").expect("write reader");

    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/markdown-lock-perf"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let writer_uri = "axiom://resources/markdown-lock-perf/writer.md";
    let reader_uri = "axiom://resources/markdown-lock-perf/reader.md";

    let load_samples = 80usize;
    let mut baseline_load_us = Vec::<u128>::with_capacity(load_samples);
    for _ in 0..load_samples {
        let started = std::time::Instant::now();
        app.load_markdown(reader_uri).expect("baseline load");
        baseline_load_us.push(started.elapsed().as_micros());
    }

    let writer_iterations = 120usize;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let writer_barrier = std::sync::Arc::clone(&barrier);
    let writer_app = app.clone();
    let writer_handle = std::thread::spawn(move || {
        writer_barrier.wait();
        for idx in 0..writer_iterations {
            let content = format!("# Writer\n\nload contention iteration {idx}");
            writer_app
                .save_markdown(writer_uri, &content, None)
                .expect("writer save");
        }
    });

    barrier.wait();
    let mut contended_load_us = Vec::<u128>::with_capacity(load_samples);
    for _ in 0..load_samples {
        let started = std::time::Instant::now();
        app.load_markdown(reader_uri)
            .expect("contended reader load");
        contended_load_us.push(started.elapsed().as_micros());
    }
    writer_handle.join().expect("writer join");

    let baseline_p95_us = percentile_p95(&baseline_load_us);
    let contended_p95_us = percentile_p95(&contended_load_us);
    let amplification = contended_p95_us as f64 / baseline_p95_us.max(1) as f64;
    println!(
        "markdown_lock_contention_reference baseline_load_p95_us={baseline_p95_us} contended_load_p95_us={contended_p95_us} amplification_x={amplification:.2}"
    );
}

fn percentile_p95(values: &[u128]) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mut index = (sorted.len() * 95).div_ceil(100);
    index = index.saturating_sub(1).min(sorted.len().saturating_sub(1));
    sorted[index]
}
