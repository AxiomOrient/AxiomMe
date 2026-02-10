use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use tempfile::tempdir;

use crate::catalog::eval_golden_uri;
use crate::models::{
    BenchmarkGateOptions, BenchmarkRunOptions, EvalRunOptions, MetadataFilter, QueryPlan,
    ReconcileOptions, ReleaseCheckDocument, TraceIndexEntry,
};
use crate::queue_policy::retry_backoff_seconds;
use crate::release_gate::{evaluate_contract_integrity_gate, resolve_workspace_dir};
use crate::{AxiomError, AxiomUri, Scope};
use chrono::Utc;

use super::AxiomMe;

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
fn backend_status_exposes_embedding_profile() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let status = app.backend_status().expect("backend status");
    let profile = crate::embedding::embedding_profile();
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
fn markdown_editor_rolls_back_file_content_when_reindex_fails() {
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

    let err = app
        .save_markdown(uri, "# Guide\n\nrollback_new_token", Some(&loaded.etag))
        .expect_err("save must fail");
    assert!(matches!(err, AxiomError::Internal(_)));

    let after = app.load_markdown(uri).expect("load after failed save");
    assert!(after.content.contains("rollback_old_token"));
    assert!(!after.content.contains("rollback_new_token"));
}

#[test]
fn find_and_search_apply_metadata_filters() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("filter_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(corpus_dir.join("auth.md"), "OAuth auth flow and api").expect("write auth");
    fs::write(corpus_dir.join("storage.json"), "{\"storage\":true}").expect("write storage");

    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/filter-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let mut tag_fields = HashMap::new();
    tag_fields.insert("tags".to_string(), serde_json::json!(["auth"]));
    let tag_filter = MetadataFilter { fields: tag_fields };

    let find_by_tag = app
        .find(
            "flow",
            Some("axiom://resources/filter-demo"),
            Some(10),
            None,
            Some(tag_filter.clone()),
        )
        .expect("find by tag");
    assert!(
        find_by_tag
            .query_results
            .iter()
            .any(|x| x.uri.ends_with("auth.md"))
    );
    assert!(
        !find_by_tag
            .query_results
            .iter()
            .any(|x| x.uri.ends_with("storage.json"))
    );

    let mut mime_fields = HashMap::new();
    mime_fields.insert("mime".to_string(), serde_json::json!("text/markdown"));
    let mime_filter = MetadataFilter {
        fields: mime_fields,
    };

    let search_by_mime = app
        .search(
            "flow",
            Some("axiom://resources/filter-demo"),
            None,
            Some(10),
            None,
            Some(mime_filter),
        )
        .expect("search by mime");
    assert!(
        search_by_mime
            .query_results
            .iter()
            .any(|x| x.uri.ends_with("auth.md"))
    );
    assert!(
        !search_by_mime
            .query_results
            .iter()
            .any(|x| x.uri.ends_with("storage.json"))
    );
}

#[test]
fn contract_execution_probe_validates_core_algorithms() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("contract_probe_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(
        corpus_dir.join("auth.md"),
        "OAuth authorization code flow and token exchange",
    )
    .expect("write auth");
    fs::write(
        corpus_dir.join("storage.json"),
        "{\"storage\": \"local\", \"cache\": true}",
    )
    .expect("write storage");

    app.add_resource(
        corpus_dir.to_str().expect("corpus str"),
        Some("axiom://resources/contract-probe"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let search = app
        .search(
            "oauth authorization code flow",
            Some("axiom://resources/contract-probe"),
            None,
            Some(10),
            None,
            None,
        )
        .expect("search");
    assert!(search.query_plan["typed_queries"].is_array());
    assert!(!search.query_results.is_empty());

    let mut tag_fields = HashMap::new();
    tag_fields.insert("tags".to_string(), serde_json::json!(["auth"]));
    let filtered = app
        .find(
            "flow",
            Some("axiom://resources/contract-probe"),
            Some(10),
            None,
            Some(MetadataFilter { fields: tag_fields }),
        )
        .expect("filtered find");
    assert!(!filtered.query_results.is_empty());
    assert!(
        filtered
            .query_results
            .iter()
            .all(|hit| hit.uri.ends_with("auth.md"))
    );

    app.link(
        "axiom://resources/contract-probe",
        "probe-link",
        vec![
            "axiom://resources/contract-probe/auth.md".to_string(),
            "axiom://resources/contract-probe/storage.json".to_string(),
        ],
        "contract-probe relation",
    )
    .expect("link");
    let relation_find = app
        .find(
            "oauth",
            Some("axiom://resources/contract-probe"),
            Some(10),
            None,
            None,
        )
        .expect("relation find");
    assert!(
        relation_find
            .query_results
            .iter()
            .any(|hit| !hit.relations.is_empty())
    );

    let session = app.session(Some("contract-probe-session"));
    session.load().expect("session load");
    session
        .add_message("user", "My name is contract probe")
        .expect("profile");
    session
        .add_message("user", "I prefer concise Rust code")
        .expect("preferences");
    session
        .add_message("user", "This project repository is AxiomMe")
        .expect("entities");
    session
        .add_message("assistant", "Today we deployed release candidate")
        .expect("events");
    session
        .add_message("assistant", "Root cause fixed with workaround")
        .expect("cases");
    session
        .add_message("assistant", "Always run this checklist before release")
        .expect("patterns");
    let commit = session.commit().expect("commit");
    assert!(commit.memories_extracted >= 6);

    let memory_find = app
        .find(
            "concise Rust code",
            Some("axiom://user/memories/preferences"),
            Some(5),
            None,
            None,
        )
        .expect("memory find");
    assert!(!memory_find.query_results.is_empty());
}

#[test]
fn search_uses_archive_relevant_session_hints_when_active_messages_absent() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let session = app.session(Some("s-archive-hints"));
    session.load().expect("load failed");
    session
        .add_message("user", "OAuth archive hint for token refresh")
        .expect("append");
    session.commit().expect("commit");

    let result = app
        .search(
            "refresh",
            Some("axiom://resources"),
            Some("s-archive-hints"),
            Some(10),
            None,
            None,
        )
        .expect("search");

    let plan: QueryPlan = serde_json::from_value(result.query_plan).expect("query plan");
    let session_query = plan
        .typed_queries
        .iter()
        .find(|x| x.kind == "session_recent")
        .map(|x| x.query.to_lowercase())
        .unwrap_or_default();
    assert!(session_query.contains("oauth archive hint"));
}

#[test]
fn relation_api_supports_link_unlink_and_list_crud() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let owner = "axiom://resources/relation-demo";
    let created = app
        .link(
            owner,
            "auth-security",
            vec![
                "axiom://resources/relation-demo/auth".to_string(),
                "axiom://resources/relation-demo/security".to_string(),
            ],
            "Security dependency",
        )
        .expect("link create");
    assert_eq!(created.id, "auth-security");

    let listed = app.relations(owner).expect("relations list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "auth-security");
    assert_eq!(listed[0].reason, "Security dependency");

    let updated = app
        .link(
            owner,
            "auth-security",
            vec![
                "axiom://resources/relation-demo/auth".to_string(),
                "axiom://resources/relation-demo/security".to_string(),
            ],
            "Updated dependency rationale",
        )
        .expect("link update");
    assert_eq!(updated.id, "auth-security");
    assert_eq!(updated.reason, "Updated dependency rationale");

    let listed_after_update = app.relations(owner).expect("relations list updated");
    assert_eq!(listed_after_update.len(), 1);
    assert_eq!(
        listed_after_update[0].reason,
        "Updated dependency rationale"
    );

    let removed = app.unlink(owner, "auth-security").expect("unlink existing");
    assert!(removed);
    assert!(app.relations(owner).expect("relations empty").is_empty());

    let removed_missing = app.unlink(owner, "auth-security").expect("unlink missing");
    assert!(!removed_missing);
}

#[test]
fn relation_api_rejects_queue_scope_write_for_link() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let err = app
        .link(
            "axiom://queue/relation-demo",
            "q-link",
            vec![
                "axiom://resources/demo/a".to_string(),
                "axiom://resources/demo/b".to_string(),
            ],
            "queue should be readonly",
        )
        .expect_err("link must fail on queue");
    assert!(matches!(err, AxiomError::PermissionDenied(_)));
}

#[test]
fn find_and_search_enrich_hits_with_relations() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("relation_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(
        corpus_dir.join("auth.md"),
        "OAuth auth flow and token rotation",
    )
    .expect("write");
    fs::write(
        corpus_dir.join("security.md"),
        "Security baseline and hardening notes",
    )
    .expect("write");
    app.add_resource(
        corpus_dir.to_str().expect("corpus"),
        Some("axiom://resources/relation-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add");

    app.link(
        "axiom://resources/relation-demo",
        "auth-security",
        vec![
            "axiom://resources/relation-demo/auth.md".to_string(),
            "axiom://resources/relation-demo/security.md".to_string(),
        ],
        "Security dependency",
    )
    .expect("link");

    let find = app
        .find(
            "oauth",
            Some("axiom://resources/relation-demo"),
            Some(10),
            None,
            None,
        )
        .expect("find");
    assert!(
        find.query_results
            .iter()
            .any(|hit| hit.uri.ends_with("auth.md") && !hit.relations.is_empty())
    );
    assert!(find.query_results.iter().any(|hit| {
        hit.uri.ends_with("auth.md")
            && hit
                .relations
                .iter()
                .any(|rel| rel.uri.ends_with("security.md"))
    }));
    let find_trace = find.trace.as_ref().expect("find trace");
    assert!(find_trace.metrics.typed_query_count >= 1);
    assert!(find_trace.metrics.relation_enriched_hits >= 1);
    assert!(find_trace.metrics.relation_enriched_links >= 1);

    let search = app
        .search(
            "security oauth",
            Some("axiom://resources/relation-demo"),
            None,
            Some(10),
            None,
            None,
        )
        .expect("search");
    assert!(
        search
            .query_results
            .iter()
            .any(|hit| !hit.relations.is_empty())
    );
    let search_trace = search.trace.as_ref().expect("search trace");
    assert!(search_trace.metrics.typed_query_count >= 1);
    assert!(search_trace.metrics.relation_enriched_hits >= 1);
}

#[test]
fn find_soft_fails_when_relations_file_is_corrupted() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus_dir = temp.path().join("relation_corrupt_corpus");
    fs::create_dir_all(&corpus_dir).expect("mkdir");
    fs::write(
        corpus_dir.join("auth.md"),
        "OAuth auth flow and token rotation",
    )
    .expect("write");
    app.add_resource(
        corpus_dir.to_str().expect("corpus"),
        Some("axiom://resources/relation-corrupt-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add");

    let corrupt = AxiomUri::parse("axiom://resources/relation-corrupt-demo/.relations.json")
        .expect("parse uri");
    app.fs
        .write(&corrupt, "{invalid-json", true)
        .expect("write corrupt relations");

    let result = app
        .find(
            "oauth",
            Some("axiom://resources/relation-corrupt-demo"),
            Some(10),
            None,
            None,
        )
        .expect("find should not fail");
    assert!(!result.query_results.is_empty());
}

#[test]
fn find_persists_trace_and_supports_replay_lookup() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("trace_input.txt");
    fs::write(&src, "OAuth flow trace coverage.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/trace-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let result = app
        .find(
            "oauth",
            Some("axiom://resources/trace-demo"),
            Some(5),
            None,
            None,
        )
        .expect("find failed");

    let trace = result.trace.expect("trace missing");
    let trace_uri = result.trace_uri.expect("trace_uri missing");
    let parsed_uri = AxiomUri::parse(&trace_uri).expect("trace uri parse");
    assert!(app.fs.exists(&parsed_uri));

    let fetched = app
        .get_trace(&trace.trace_id)
        .expect("get trace")
        .expect("trace not found");
    assert_eq!(fetched.trace_id, trace.trace_id);
    assert_eq!(fetched.request_type, "find");

    let listed = app.list_traces(10).expect("list traces");
    assert!(
        listed
            .iter()
            .any(|entry| entry.trace_id == trace.trace_id && entry.uri == trace_uri)
    );
}

#[test]
fn replay_trace_reexecutes_query_and_persists_new_trace() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("replay_trace_input.txt");
    fs::write(&src, "OAuth replay trace flow.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/replay-trace-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let original = app
        .find(
            "oauth",
            Some("axiom://resources/replay-trace-demo"),
            Some(5),
            None,
            None,
        )
        .expect("find failed");
    let original_trace_id = original
        .trace
        .as_ref()
        .map(|t| t.trace_id.clone())
        .expect("trace missing");

    let replay = app
        .replay_trace(&original_trace_id, Some(3))
        .expect("replay failed")
        .expect("replay missing");
    assert!(!replay.query_results.is_empty());
    assert!(replay.trace_uri.is_some());
    assert!(
        replay
            .trace
            .as_ref()
            .map(|t| t.request_type.ends_with("_replay"))
            .unwrap_or(false)
    );
}

#[test]
fn request_logs_include_request_and_trace_ids_for_find() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("request_log_find_input.txt");
    fs::write(&src, "OAuth request log find flow.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/request-log-find"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/request-log-find"),
            Some(5),
            None,
            None,
        )
        .expect("find failed");

    let logs = app.list_request_logs(50).expect("list logs");
    let entry = logs
        .iter()
        .find(|x| x.operation == "find" && x.status == "ok")
        .expect("find log missing");
    assert!(!entry.request_id.trim().is_empty());
    assert!(entry.trace_id.is_some());
    assert_eq!(entry.error_code, None);
}

#[test]
fn request_logs_capture_errors_for_invalid_find_target() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let err = app
        .find("oauth", Some("invalid://bad-target"), Some(5), None, None)
        .expect_err("find should fail");
    assert!(matches!(err, AxiomError::InvalidUri(_)));

    let logs = app.list_request_logs(50).expect("list logs");
    let entry = logs
        .iter()
        .find(|x| x.operation == "find" && x.status == "error")
        .expect("error log missing");
    assert_eq!(entry.error_code.as_deref(), Some("INVALID_URI"));
    assert!(entry.trace_id.is_none());
}

#[test]
fn request_logs_support_operation_status_filters_case_insensitive() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let _ = app.replay_outbox(10, false).expect("replay");
    let _ = app
        .find("oauth", Some("invalid://bad-target"), Some(5), None, None)
        .expect_err("find should fail");

    let replay_logs = app
        .list_request_logs_filtered(20, Some("QUEUE.REPLAY"), Some("OK"))
        .expect("list replay logs");
    assert!(!replay_logs.is_empty());
    assert!(
        replay_logs
            .iter()
            .all(|entry| entry.operation == "queue.replay" && entry.status == "ok")
    );

    let find_errors = app
        .list_request_logs_filtered(20, Some("FiNd"), Some("ErRoR"))
        .expect("list find errors");
    assert!(!find_errors.is_empty());
    assert!(
        find_errors
            .iter()
            .all(|entry| entry.operation == "find" && entry.status == "error")
    );
}

#[test]
fn request_logs_capture_extended_core_operations() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("request_log_extended_input.txt");
    fs::write(&src, "OAuth request log extended operations flow.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/request-log-extended"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/request-log-extended"),
            Some(5),
            None,
            None,
        )
        .expect("find failed");

    let eval = app
        .run_eval_loop_with_options(EvalRunOptions {
            trace_limit: 20,
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            golden_only: false,
        })
        .expect("eval");
    assert!(eval.executed_cases >= 1);

    let benchmark = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 20,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("benchmark");
    assert!(benchmark.executed_cases >= 1);

    let gate = app
        .benchmark_gate_with_options(BenchmarkGateOptions {
            gate_profile: "unit-test".to_string(),
            threshold_p95_ms: u128::MAX,
            min_top1_accuracy: 0.0,
            max_p95_regression_pct: None,
            max_top1_regression_pct: None,
            window_size: 1,
            required_passes: 1,
            record: false,
            write_release_check: false,
        })
        .expect("gate");
    assert!(gate.passed);

    let reconcile = app
        .reconcile_state_with_options(ReconcileOptions {
            dry_run: true,
            scopes: Some(vec![Scope::Resources]),
            max_drift_sample: 20,
        })
        .expect("reconcile");
    assert_eq!(reconcile.status, "dry_run");

    let export_base = temp.path().join("request-log-extended-pack");
    let export_path = app
        .export_ovpack(
            "axiom://resources/request-log-extended",
            export_base.to_str().expect("export path"),
        )
        .expect("export ovpack");
    let imported = app
        .import_ovpack(&export_path, "axiom://resources/import-root", true, false)
        .expect("import ovpack");
    assert!(
        imported.starts_with("axiom://resources/import-root/request-log-extended"),
        "unexpected imported uri: {}",
        imported
    );

    let logs = app.list_request_logs(300).expect("list logs");
    for operation in [
        "add_resource",
        "eval.run",
        "benchmark.run",
        "benchmark.gate",
        "reconcile.run",
        "ovpack.export",
        "ovpack.import",
    ] {
        assert!(
            logs.iter()
                .any(|entry| entry.operation == operation && entry.status != "error"),
            "missing request log for operation {}",
            operation
        );
    }

    let dry_run_logs = app
        .list_request_logs_filtered(20, Some("reconcile.run"), Some("dry_run"))
        .expect("filter reconcile");
    assert!(!dry_run_logs.is_empty());
}

#[test]
fn security_audit_generates_report_artifact() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let report = app
        .run_security_audit(Some(workspace.to_str().expect("workspace str")))
        .expect("security audit");
    assert!(
        report
            .report_uri
            .starts_with("axiom://queue/release/security/")
    );
    let report_uri = AxiomUri::parse(&report.report_uri).expect("uri parse");
    assert!(app.fs.exists(&report_uri));
    assert_eq!(report.dependency_audit.tool, "cargo-audit");
    assert!(
        report.dependency_audit.status == "passed"
            || report.dependency_audit.status == "vulnerabilities_found"
            || report.dependency_audit.status == "tool_missing"
            || report.dependency_audit.status == "error"
    );
    assert!(!report.checks.is_empty());
}

#[test]
fn operability_evidence_generates_report_artifact() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("operability_evidence_input.txt");
    fs::write(&src, "operability evidence query source").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/operability-evidence"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "operability",
            Some("axiom://resources/operability-evidence"),
            Some(5),
            None,
            None,
        )
        .expect("find failed");

    let report = app
        .collect_operability_evidence(50, 50)
        .expect("collect operability evidence");
    assert!(
        report
            .report_uri
            .starts_with("axiom://queue/release/operability/")
    );
    assert!(
        report
            .trace_metrics_snapshot_uri
            .starts_with("axiom://queue/metrics/traces/snapshots/")
    );
    assert!(report.traces_analyzed >= 1);
    assert!(report.request_logs_scanned >= 1);
    assert!(!report.checks.is_empty());
    let report_uri = AxiomUri::parse(&report.report_uri).expect("uri parse");
    assert!(app.fs.exists(&report_uri));
}

#[test]
fn reliability_evidence_generates_report_artifact() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let report = app
        .collect_reliability_evidence(100, 8)
        .expect("collect reliability evidence");
    assert!(
        report
            .report_uri
            .starts_with("axiom://queue/release/reliability/")
    );
    assert!(report.passed, "report must pass: {:?}", report.checks);
    assert!(report.replay_totals.done >= 1);
    assert!(report.replay_hit_uri.is_some());
    assert!(report.restart_hit_uri.is_some());
    let report_uri = AxiomUri::parse(&report.report_uri).expect("uri parse");
    assert!(app.fs.exists(&report_uri));
}

#[test]
fn release_benchmark_seed_trace_generates_find_trace() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let before = app.list_traces(20).expect("list traces before").len();
    app.ensure_release_benchmark_seed_trace()
        .expect("ensure seed trace");
    let after = app.list_traces(20).expect("list traces after");
    assert!(after.len() > before);
    assert!(after.iter().any(|entry| entry.request_type == "find"));
}

#[test]
fn resolve_workspace_dir_requires_manifest() {
    let temp = tempdir().expect("tempdir");
    let err = resolve_workspace_dir(Some(temp.path().to_str().expect("temp path str")))
        .expect_err("must fail");
    assert!(matches!(err, AxiomError::Validation(_)));
}

#[test]
fn contract_integrity_gate_detects_missing_markers() {
    let temp = tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("docs")).expect("mkdir docs");
    fs::create_dir_all(temp.path().join("plan")).expect("mkdir plan");

    fs::write(
        temp.path().join("docs").join("API_CONTRACT.md"),
        "# API\n\n- no extension markers\n",
    )
    .expect("write api");
    fs::write(
        temp.path().join("docs").join("FEATURE_SPEC.md"),
        "# Feature\n",
    )
    .expect("write feature");
    fs::write(temp.path().join("plan").join("TASKS.md"), "# Tasks\n").expect("write tasks");
    fs::write(
        temp.path().join("plan").join("QUALITY_GATES.md"),
        "# Gates\n",
    )
    .expect("write gates");

    let decision = evaluate_contract_integrity_gate(temp.path());
    assert_eq!(decision.gate_id, "G0");
    assert!(!decision.passed);
    assert!(decision.details.contains("missing_marker"));
}

#[test]
fn export_ovpack_rejects_internal_scope_source() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let queue_uri = AxiomUri::parse("axiom://queue/export-test").expect("parse uri");
    app.fs
        .create_dir_all(&queue_uri, true)
        .expect("create queue dir");
    app.fs
        .write(&queue_uri.join("note.txt").expect("join"), "internal", true)
        .expect("write queue file");

    let out_path = temp.path().join("queue_export");
    let err = app
        .export_ovpack(
            "axiom://queue/export-test",
            out_path.to_str().expect("out str"),
        )
        .expect_err("must fail");
    assert!(matches!(err, AxiomError::PermissionDenied(_)));
}

#[test]
fn import_ovpack_rejects_internal_scope_parent() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("ovpack_input.txt");
    fs::write(&src, "OAuth ovpack import scope guard").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/import-guard-src"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let pack_file = temp.path().join("import_guard_pack");
    let exported = app
        .export_ovpack(
            "axiom://resources/import-guard-src",
            pack_file.to_str().expect("pack path"),
        )
        .expect("export");

    let err = app
        .import_ovpack(&exported, "axiom://queue/import-guard", true, false)
        .expect_err("must fail");
    assert!(matches!(err, AxiomError::PermissionDenied(_)));
}

#[test]
fn trace_metrics_support_replay_filtering_and_request_type_breakdown() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("trace_metrics_input.txt");
    fs::write(&src, "OAuth trace metrics coverage.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/trace-metrics-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let find_result = app
        .find(
            "oauth",
            Some("axiom://resources/trace-metrics-demo"),
            Some(5),
            None,
            None,
        )
        .expect("find failed");
    let source_trace_id = find_result
        .trace
        .as_ref()
        .map(|x| x.trace_id.clone())
        .expect("find trace missing");

    let _ = app
        .search(
            "oauth flow",
            Some("axiom://resources/trace-metrics-demo"),
            None,
            Some(5),
            None,
            None,
        )
        .expect("search failed");
    let _ = app
        .replay_trace(&source_trace_id, Some(3))
        .expect("replay failed");

    let stats_no_replay = app.trace_metrics(50, false).expect("metrics");
    assert!(stats_no_replay.traces_analyzed >= 2);
    assert_eq!(stats_no_replay.traces_skipped_invalid, 0);
    assert!(
        stats_no_replay
            .by_request_type
            .iter()
            .any(|x| x.request_type == "find")
    );
    assert!(
        stats_no_replay
            .by_request_type
            .iter()
            .any(|x| x.request_type == "search")
    );
    assert!(
        stats_no_replay
            .by_request_type
            .iter()
            .all(|x| !x.request_type.ends_with("_replay"))
    );

    let stats_with_replay = app.trace_metrics(50, true).expect("metrics replay");
    assert!(stats_with_replay.traces_analyzed >= stats_no_replay.traces_analyzed);
    assert_eq!(stats_with_replay.traces_skipped_invalid, 0);
    assert!(
        stats_with_replay
            .by_request_type
            .iter()
            .any(|x| x.request_type.ends_with("_replay"))
    );
}

#[test]
fn trace_metrics_skips_invalid_trace_payloads() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let bogus_uri = AxiomUri::parse("axiom://queue/traces/bogus.json").expect("uri parse");
    app.fs
        .write(&bogus_uri, "{not-json", true)
        .expect("write bogus trace");
    app.state
        .upsert_trace_index(&TraceIndexEntry {
            trace_id: "bogus-trace".to_string(),
            uri: bogus_uri.to_string(),
            request_type: "find".to_string(),
            query: "oauth".to_string(),
            target_uri: None,
            created_at: Utc::now().to_rfc3339(),
        })
        .expect("upsert trace index");

    let stats = app.trace_metrics(10, true).expect("trace metrics");
    assert_eq!(stats.indexed_traces_scanned, 1);
    assert_eq!(stats.traces_analyzed, 0);
    assert_eq!(stats.traces_skipped_missing, 0);
    assert_eq!(stats.traces_skipped_invalid, 1);
}

#[test]
fn trace_metrics_snapshot_list_and_trend_workflow() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("trace_metrics_snapshot_input.txt");
    fs::write(&src, "OAuth trace metrics snapshot coverage.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/trace-metrics-snapshot-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let find_result = app
        .find(
            "oauth",
            Some("axiom://resources/trace-metrics-snapshot-demo"),
            Some(5),
            None,
            None,
        )
        .expect("find failed");
    let source_trace_id = find_result
        .trace
        .as_ref()
        .map(|x| x.trace_id.clone())
        .expect("trace missing");

    let first = app
        .create_trace_metrics_snapshot(50, false)
        .expect("create first snapshot");
    let first_uri = AxiomUri::parse(&first.report_uri).expect("first report uri parse");
    assert!(app.fs.exists(&first_uri));

    let _ = app
        .search(
            "oauth flow",
            Some("axiom://resources/trace-metrics-snapshot-demo"),
            None,
            Some(5),
            None,
            None,
        )
        .expect("search failed");
    let _ = app
        .replay_trace(&source_trace_id, Some(3))
        .expect("replay failed");

    let second = app
        .create_trace_metrics_snapshot(50, true)
        .expect("create second snapshot");
    let second_uri = AxiomUri::parse(&second.report_uri).expect("second report uri parse");
    assert!(app.fs.exists(&second_uri));

    let snapshots = app
        .list_trace_metrics_snapshots(20)
        .expect("list snapshots");
    assert!(snapshots.len() >= 2);
    assert!(snapshots.iter().any(|x| x.snapshot_id == first.snapshot_id));
    assert!(
        snapshots
            .iter()
            .any(|x| x.snapshot_id == second.snapshot_id)
    );

    let trend = app
        .trace_metrics_trend(20, Some("FIND"))
        .expect("trace trend");
    assert_eq!(trend.request_type, "find");
    assert!(trend.latest.is_some());
    assert!(trend.previous.is_some());
}

#[test]
fn trace_metrics_trend_reports_no_data_without_snapshots() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let trend = app.trace_metrics_trend(20, Some("find")).expect("trend");
    assert_eq!(trend.status, "no_data");
    assert!(trend.latest.is_none());
    assert!(trend.previous.is_none());
    assert!(trend.delta_p95_latency_ms.is_none());
}

#[test]
fn eval_loop_generates_report_and_query_set_artifacts() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("eval_loop_input.txt");
    fs::write(&src, "OAuth eval loop query coverage.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/eval-loop-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/eval-loop-demo"),
            Some(5),
            None,
            None,
        )
        .expect("find failed");

    let report = app.run_eval_loop(20, 10, 5).expect("run eval loop");
    assert!(report.traces_scanned >= 1);
    assert!(report.executed_cases >= 1);
    assert_eq!(report.passed + report.failed, report.executed_cases);

    let report_uri = AxiomUri::parse(&report.report_uri).expect("report uri");
    let query_set_uri = AxiomUri::parse(&report.query_set_uri).expect("query set uri");
    let markdown_report_uri =
        AxiomUri::parse(&report.markdown_report_uri).expect("markdown report uri");
    assert!(app.fs.exists(&report_uri));
    assert!(app.fs.exists(&query_set_uri));
    assert!(app.fs.exists(&markdown_report_uri));
}

#[test]
fn eval_loop_emits_required_failure_bucket_metrics() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("eval_bucket_probe_input.txt");
    fs::write(&src, "OAuth eval required bucket coverage.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/eval-bucket-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/eval-bucket-demo"),
            Some(5),
            None,
            None,
        )
        .expect("find failed");

    let report = app.run_eval_loop(20, 10, 5).expect("run eval loop");
    for name in [
        "intent_miss",
        "filter_ignored",
        "memory_category_miss",
        "archive_context_miss",
        "relation_missing",
    ] {
        assert!(
            report.buckets.iter().any(|bucket| bucket.name == name),
            "missing required bucket metric: {}",
            name
        );
    }
}

#[test]
fn eval_golden_queries_support_add_and_golden_only_run() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("eval_golden_input.txt");
    fs::write(&src, "OAuth golden query coverage.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/eval-golden-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let find = app
        .find(
            "oauth",
            Some("axiom://resources/eval-golden-demo"),
            Some(5),
            None,
            None,
        )
        .expect("find failed");
    let expected = find
        .query_results
        .first()
        .map(|x| x.uri.clone())
        .expect("missing expected top");

    let add = app
        .add_eval_golden_query(
            "oauth",
            Some("axiom://resources/eval-golden-demo"),
            Some(&expected),
        )
        .expect("add golden");
    assert!(add.count >= 1);

    let report = app
        .run_eval_loop_with_options(EvalRunOptions {
            trace_limit: 20,
            query_limit: 10,
            search_limit: 5,
            include_golden: true,
            golden_only: true,
        })
        .expect("run golden only");
    assert!(report.golden_cases_used >= 1);
    assert_eq!(report.trace_cases_used, 0);
    assert!(report.include_golden);
    assert!(report.golden_only);
}

#[test]
fn eval_golden_merge_from_traces_is_idempotent() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("eval_merge_input.txt");
    fs::write(&src, "OAuth merge seed coverage.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/eval-merge-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/eval-merge-demo"),
            Some(5),
            None,
            None,
        )
        .expect("find failed");

    let first = app
        .merge_eval_golden_from_traces(50, 20)
        .expect("merge first");
    assert!(first.added_count >= 1);
    assert_eq!(first.after_count, first.before_count + first.added_count);

    let second = app
        .merge_eval_golden_from_traces(50, 20)
        .expect("merge second");
    assert_eq!(second.added_count, 0);
}

#[test]
fn eval_golden_add_without_expected_does_not_clear_existing_expectation() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let expected = "axiom://resources/demo/file.md";
    let _ = app
        .add_eval_golden_query("oauth", Some("axiom://resources/demo"), Some(expected))
        .expect("add with expected");
    let _ = app
        .add_eval_golden_query("oauth", Some("axiom://resources/demo"), None)
        .expect("add without expected");

    let cases = app.list_eval_golden_queries().expect("list cases");
    let case = cases
        .iter()
        .find(|c| c.query == "oauth" && c.target_uri.as_deref() == Some("axiom://resources/demo"))
        .expect("missing case");
    assert_eq!(case.expected_top_uri.as_deref(), Some(expected));
}

#[test]
fn eval_golden_loader_rejects_legacy_array_format() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let golden_uri = eval_golden_uri().expect("golden uri");
    app.fs
            .write(
                &golden_uri,
                r#"[{"source_trace_id":"legacy","query":"oauth","target_uri":"axiom://resources/demo","expected_top_uri":"axiom://resources/demo/file.md"}]"#,
                true,
            )
            .expect("write legacy payload");

    let err = app
        .list_eval_golden_queries()
        .expect_err("must reject legacy array format");
    assert_eq!(err.code(), "JSON_ERROR");
}

#[test]
fn benchmark_suite_generates_report_and_artifacts() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("benchmark_input.txt");
    fs::write(&src, "OAuth benchmark suite content.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-demo"),
            Some(5),
            None,
            None,
        )
        .expect("find");

    let report = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 20,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("benchmark");
    assert!(report.executed_cases >= 1);
    assert!(report.p95_latency_ms >= report.p50_latency_ms);

    let report_uri = AxiomUri::parse(&report.report_uri).expect("report uri");
    let markdown_uri = AxiomUri::parse(&report.markdown_report_uri).expect("markdown uri");
    let case_set_uri = AxiomUri::parse(&report.case_set_uri).expect("set uri");
    assert!(app.fs.exists(&report_uri));
    assert!(app.fs.exists(&markdown_uri));
    assert!(app.fs.exists(&case_set_uri));
}

#[test]
fn benchmark_report_includes_protocol_metadata_and_acceptance_mapping() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_protocol_input.txt");
    fs::write(&src, "OAuth benchmark protocol metadata content.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-protocol"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-protocol"),
            Some(5),
            None,
            None,
        )
        .expect("find");

    let report = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 20,
            search_limit: 10,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("benchmark");

    assert!(!report.environment.machine_profile.trim().is_empty());
    assert!(!report.environment.cpu_model.trim().is_empty());
    assert!(!report.environment.os_version.trim().is_empty());
    assert!(!report.environment.rustc_version.trim().is_empty());
    assert_eq!(report.environment.retrieval_backend, "sqlite");
    assert_eq!(report.environment.reranker_profile, "doc-aware-v1");
    assert!(report.corpus.snapshot_id.starts_with("resources-"));
    assert!(report.query_set.version.starts_with("qset-v1-"));
    assert_eq!(
        report.acceptance.measured.total_queries,
        report.query_set.total_queries
    );
    assert_eq!(report.acceptance.protocol_id, "macmini-g6-v1");
    assert!(!report.acceptance.checks.is_empty());
    assert!(report.p99_latency_ms >= report.p95_latency_ms);
    assert!(report.search_p99_latency_ms >= report.search_p95_latency_ms);
    assert!(report.commit_p99_latency_ms >= report.commit_p95_latency_ms);
}

#[test]
fn benchmark_results_include_expected_rank_for_expected_cases() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_rank_input.txt");
    fs::write(&src, "OAuth benchmark expected rank coverage.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-rank"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-rank"),
            Some(10),
            None,
            None,
        )
        .expect("find");

    let report = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 20,
            search_limit: 10,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("benchmark");
    assert!(report.results.iter().any(|x| x.expected_top_uri.is_some()));
    assert!(
        report
            .results
            .iter()
            .filter(|x| x.expected_top_uri.is_some())
            .all(|x| x.expected_rank.is_some())
    );
}

#[test]
fn benchmark_suite_requires_at_least_one_source() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let err = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: false,
            fixture_name: None,
        })
        .expect_err("must fail");
    assert!(matches!(err, AxiomError::Validation(_)));
}

#[test]
fn benchmark_gate_fails_without_reports() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let gate = app
        .benchmark_gate(600, 0.75, Some(20.0), None)
        .expect("gate check");
    assert!(!gate.passed);
    assert!(gate.reasons.iter().any(|r| r == "no_benchmark_reports"));
}

#[test]
fn benchmark_list_and_trend_return_recent_reports() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_trend_input.txt");
    fs::write(&src, "OAuth benchmark trend content.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-trend"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-trend"),
            Some(5),
            None,
            None,
        )
        .expect("find");

    let _ = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("bench 1");
    let _ = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("bench 2");

    let list = app.list_benchmark_reports(10).expect("list");
    assert!(list.len() >= 2);

    let trend = app.benchmark_trend(10).expect("trend");
    assert!(trend.latest.is_some());
    assert!(trend.previous.is_some());
    assert!(trend.delta_p95_latency_ms.is_some());
    assert!(trend.delta_top1_accuracy.is_some());
}

#[test]
fn benchmark_gate_enforces_thresholds() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_gate_input.txt");
    fs::write(&src, "OAuth benchmark gate content.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-gate"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-gate"),
            Some(5),
            None,
            None,
        )
        .expect("find");

    let _ = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("benchmark");

    let strict = app.benchmark_gate(0, 1.1, None, None).expect("strict gate");
    assert!(!strict.passed);

    let relaxed = app
        .benchmark_gate(10_000, 0.0, None, None)
        .expect("relaxed gate");
    assert!(relaxed.passed);
}

#[test]
fn benchmark_gate_enforces_top1_regression_threshold() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_top1_regression_input.txt");
    fs::write(&src, "OAuth benchmark top1 regression content.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-top1-regression"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-top1-regression"),
            Some(5),
            None,
            None,
        )
        .expect("find");

    let template = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("benchmark template");

    let mut previous = template.clone();
    previous.run_id = "top1-prev".to_string();
    previous.created_at = "2999-01-01T00:00:01Z".to_string();
    previous.top1_accuracy = 1.0;

    let mut latest = template;
    latest.run_id = "top1-latest".to_string();
    latest.created_at = "2999-01-01T00:00:02Z".to_string();
    latest.top1_accuracy = 0.4;

    let previous_uri =
        AxiomUri::parse("axiom://queue/benchmarks/reports/top1-prev.json").expect("prev uri");
    app.fs
        .write(
            &previous_uri,
            &serde_json::to_string_pretty(&previous).expect("serialize previous"),
            true,
        )
        .expect("write previous");
    let latest_uri =
        AxiomUri::parse("axiom://queue/benchmarks/reports/top1-latest.json").expect("latest");
    app.fs
        .write(
            &latest_uri,
            &serde_json::to_string_pretty(&latest).expect("serialize latest"),
            true,
        )
        .expect("write latest");

    let gate = app
        .benchmark_gate_with_options(BenchmarkGateOptions {
            gate_profile: "top1-regression-test".to_string(),
            threshold_p95_ms: 10_000,
            min_top1_accuracy: 0.0,
            max_p95_regression_pct: None,
            max_top1_regression_pct: Some(10.0),
            window_size: 1,
            required_passes: 1,
            record: false,
            write_release_check: false,
        })
        .expect("gate");

    assert!(!gate.passed);
    assert!(gate.top1_regression_pct.is_some());
    assert_eq!(gate.run_results.len(), 1);
    assert!(gate.run_results[0].top1_regression_pct.is_some());
    assert!(
        gate.run_results[0]
            .reasons
            .iter()
            .any(|r| r.starts_with("top1_regression_exceeded:"))
    );
}

#[test]
fn benchmark_gate_enforces_semantic_quality_regression_threshold() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_semantic_regression_input.txt");
    fs::write(&src, "OAuth benchmark semantic regression content.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-semantic-regression"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-semantic-regression"),
            Some(5),
            None,
            None,
        )
        .expect("find");

    let template = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("benchmark template");

    let mut previous = template.clone();
    previous.run_id = "semantic-prev".to_string();
    previous.created_at = "2999-02-01T00:00:01Z".to_string();
    previous.ndcg_at_10 = 0.92;
    previous.recall_at_10 = 0.95;
    previous.query_set.total_queries = 120;
    previous.query_set.semantic_queries = 60;
    previous.query_set.lexical_queries = 40;
    previous.query_set.mixed_queries = 20;

    let mut latest = template;
    latest.run_id = "semantic-latest".to_string();
    latest.created_at = "2999-02-01T00:00:02Z".to_string();
    latest.ndcg_at_10 = 0.80;
    latest.recall_at_10 = 0.86;
    latest.query_set.total_queries = 120;
    latest.query_set.semantic_queries = 60;
    latest.query_set.lexical_queries = 40;
    latest.query_set.mixed_queries = 20;

    let previous_uri =
        AxiomUri::parse("axiom://queue/benchmarks/reports/semantic-prev.json").expect("prev uri");
    app.fs
        .write(
            &previous_uri,
            &serde_json::to_string_pretty(&previous).expect("serialize previous"),
            true,
        )
        .expect("write previous");
    let latest_uri =
        AxiomUri::parse("axiom://queue/benchmarks/reports/semantic-latest.json").expect("latest");
    app.fs
        .write(
            &latest_uri,
            &serde_json::to_string_pretty(&latest).expect("serialize latest"),
            true,
        )
        .expect("write latest");

    let gate = app
        .benchmark_gate_with_options(BenchmarkGateOptions {
            gate_profile: "semantic-regression-test".to_string(),
            threshold_p95_ms: 10_000,
            min_top1_accuracy: 0.0,
            max_p95_regression_pct: None,
            max_top1_regression_pct: None,
            window_size: 1,
            required_passes: 1,
            record: false,
            write_release_check: false,
        })
        .expect("gate");

    assert!(!gate.passed);
    assert!(gate.run_results.iter().any(|run| {
        run.reasons
            .iter()
            .any(|r| r.starts_with("ndcg_regression_exceeded:"))
    }));
    assert!(gate.run_results.iter().any(|run| {
        run.reasons
            .iter()
            .any(|r| r.starts_with("recall_regression_exceeded:"))
    }));
}

#[test]
fn benchmark_fixture_create_list_and_run() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_fixture_input.txt");
    fs::write(&src, "OAuth benchmark fixture content.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-fixture"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-fixture"),
            Some(5),
            None,
            None,
        )
        .expect("find");

    let fixture = app
        .create_benchmark_fixture("release-smoke", 10, false, true)
        .expect("create fixture");
    assert!(fixture.case_count >= 1);

    let fixtures = app.list_benchmark_fixtures(20).expect("list fixtures");
    assert!(fixtures.iter().any(|f| f.name == "release-smoke"));

    let report = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: false,
            fixture_name: Some("release-smoke".to_string()),
        })
        .expect("run fixture benchmark");
    assert!(report.executed_cases >= 1);
}

#[test]
fn benchmark_gate_with_policy_records_result() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_policy_input.txt");
    fs::write(&src, "OAuth benchmark policy content.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-policy"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-policy"),
            Some(5),
            None,
            None,
        )
        .expect("find");

    let _ = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("bench 1");
    let _ = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("bench 2");

    let gate = app
        .benchmark_gate_with_policy(10_000, 0.0, None, 2, 2, true)
        .expect("policy gate");
    assert!(gate.passed);
    assert_eq!(gate.window_size, 2);
    assert_eq!(gate.required_passes, 2);
    assert!(gate.evaluated_runs >= 2);
    assert!(gate.passing_runs >= 2);
    assert!(gate.gate_record_uri.is_some());
    let gate_uri =
        AxiomUri::parse(gate.gate_record_uri.as_deref().expect("uri")).expect("gate uri parse");
    assert!(app.fs.exists(&gate_uri));
}

#[test]
fn benchmark_gate_with_profile_writes_release_check() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_release_check_input.txt");
    fs::write(&src, "OAuth release check content.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-release-check"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-release-check"),
            Some(5),
            None,
            None,
        )
        .expect("find");

    let _ = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("bench 1");
    let _ = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("bench 2");

    let gate = app
        .benchmark_gate_with_options(BenchmarkGateOptions {
            gate_profile: "macmini-release".to_string(),
            threshold_p95_ms: 10_000,
            min_top1_accuracy: 0.0,
            max_p95_regression_pct: None,
            max_top1_regression_pct: None,
            window_size: 2,
            required_passes: 2,
            record: true,
            write_release_check: true,
        })
        .expect("profile gate");
    assert_eq!(gate.gate_profile, "macmini-release");
    assert!(gate.gate_record_uri.is_some());
    assert!(gate.release_check_uri.is_some());

    let release_uri = AxiomUri::parse(gate.release_check_uri.as_deref().expect("uri"))
        .expect("release uri parse");
    assert!(app.fs.exists(&release_uri));
    let raw = app.fs.read(&release_uri).expect("read release check");
    let doc: ReleaseCheckDocument =
        serde_json::from_str(&raw).expect("parse release check document");
    assert_eq!(doc.gate_profile, "macmini-release");
    assert_eq!(doc.status, "pass");
    assert!(doc.gate_record_uri.is_some());
}

#[test]
fn benchmark_gate_with_policy_reports_insufficient_history() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_history_input.txt");
    fs::write(&src, "OAuth benchmark history content.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-history"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-history"),
            Some(5),
            None,
            None,
        )
        .expect("find");

    let _ = app
        .run_benchmark_suite(BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            fixture_name: None,
        })
        .expect("benchmark");

    let gate = app
        .benchmark_gate_with_policy(10_000, 0.0, None, 3, 3, false)
        .expect("gate");
    assert!(!gate.passed);
    assert!(
        gate.reasons
            .iter()
            .any(|r| r.starts_with("insufficient_history:"))
    );
}

#[test]
fn eval_failure_contains_replay_command() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("eval_fail_input.txt");
    fs::write(&src, "OAuth failure reproduction content.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/eval-fail-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let _ = app
        .add_eval_golden_query(
            "oauth",
            Some("axiom://resources/eval-fail-demo"),
            Some("axiom://resources/eval-fail-demo/wrong.md"),
        )
        .expect("add golden");

    let report = app
        .run_eval_loop_with_options(EvalRunOptions {
            trace_limit: 20,
            query_limit: 10,
            search_limit: 5,
            include_golden: true,
            golden_only: true,
        })
        .expect("eval run");
    assert!(report.failed >= 1);
    assert!(
        report
            .failures
            .iter()
            .any(|f| f.replay_command.contains("axiomme find"))
    );
}

#[test]
fn replay_outbox_marks_event_done() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let id = app
        .state
        .enqueue("delete", "axiom://resources/ghost", serde_json::json!({}))
        .expect("enqueue failed");

    let report = app.replay_outbox(10, false).expect("replay failed");
    assert_eq!(report.fetched, 1);
    assert_eq!(report.done, 1);
    assert_eq!(report.dead_letter, 0);
    assert_eq!(
        app.state.get_checkpoint("replay").expect("checkpoint"),
        Some(id)
    );
}

#[test]
fn add_resource_wait_false_requires_replay_for_searchability() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("queued.txt");
    fs::write(&src, "OAuth queued flow").expect("write queued");

    let add = app
        .add_resource(
            src.to_str().expect("src str"),
            Some("axiom://resources/queued"),
            None,
            None,
            false,
            None,
        )
        .expect("add failed");
    assert!(add.queued);

    let before = app
        .find(
            "oauth",
            Some("axiom://resources/queued"),
            Some(5),
            None,
            None,
        )
        .expect("find before");
    assert!(before.query_results.is_empty());

    let replay = app.replay_outbox(50, false).expect("replay failed");
    assert!(replay.processed >= 1);

    let after = app
        .find(
            "oauth",
            Some("axiom://resources/queued"),
            Some(5),
            None,
            None,
        )
        .expect("find after");
    assert!(!after.query_results.is_empty());
}

#[test]
fn ingest_wait_and_replay_paths_are_behaviorally_equivalent() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus = temp.path().join("ingest_equiv_corpus");
    fs::create_dir_all(corpus.join("nested")).expect("mkdir corpus");
    fs::write(
        corpus.join("auth.md"),
        "OAuth authorization code flow and token refresh",
    )
    .expect("write auth");
    fs::write(
        corpus.join("nested/storage.json"),
        "{\"storage\": \"sqlite\", \"cache\": true}",
    )
    .expect("write storage");

    app.add_resource(
        corpus.to_str().expect("corpus"),
        Some("axiom://resources/ingest-equivalence-sync"),
        None,
        None,
        true,
        None,
    )
    .expect("add sync");
    app.add_resource(
        corpus.to_str().expect("corpus"),
        Some("axiom://resources/ingest-equivalence-async"),
        None,
        None,
        false,
        None,
    )
    .expect("add async");

    let before = app
        .find(
            "oauth",
            Some("axiom://resources/ingest-equivalence-async"),
            Some(10),
            None,
            None,
        )
        .expect("find before replay");
    assert!(before.query_results.is_empty());

    let replay = app.replay_outbox(100, false).expect("replay");
    assert!(replay.processed >= 1);

    let sync_entries = app
        .ls("axiom://resources/ingest-equivalence-sync", true, false)
        .expect("ls sync");
    let async_entries = app
        .ls("axiom://resources/ingest-equivalence-async", true, false)
        .expect("ls async");

    let sync_files = sync_entries
        .iter()
        .filter(|entry| !entry.is_dir)
        .filter(|entry| !entry.name.starts_with('.'))
        .map(|entry| {
            (
                entry
                    .uri
                    .strip_prefix("axiom://resources/ingest-equivalence-sync/")
                    .unwrap_or(entry.uri.as_str())
                    .to_string(),
                app.read(&entry.uri).expect("read sync"),
            )
        })
        .collect::<Vec<_>>();
    let async_files = async_entries
        .iter()
        .filter(|entry| !entry.is_dir)
        .filter(|entry| !entry.name.starts_with('.'))
        .map(|entry| {
            (
                entry
                    .uri
                    .strip_prefix("axiom://resources/ingest-equivalence-async/")
                    .unwrap_or(entry.uri.as_str())
                    .to_string(),
                app.read(&entry.uri).expect("read async"),
            )
        })
        .collect::<Vec<_>>();

    let mut sync_files = sync_files;
    let mut async_files = async_files;
    sync_files.sort_by(|a, b| a.0.cmp(&b.0));
    async_files.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(sync_files, async_files);

    let sync_find = app
        .find(
            "oauth",
            Some("axiom://resources/ingest-equivalence-sync"),
            Some(10),
            None,
            None,
        )
        .expect("sync find");
    let async_find = app
        .find(
            "oauth",
            Some("axiom://resources/ingest-equivalence-async"),
            Some(10),
            None,
            None,
        )
        .expect("async find");
    assert!(!sync_find.query_results.is_empty());
    assert!(!async_find.query_results.is_empty());

    let done_events = app.state.fetch_outbox("done", 300).expect("done outbox");
    assert!(done_events.iter().any(|event| {
        event.event_type == "semantic_scan"
            && event.uri == "axiom://resources/ingest-equivalence-sync"
    }));
    assert!(done_events.iter().any(|event| {
        event.event_type == "semantic_scan"
            && event.uri == "axiom://resources/ingest-equivalence-async"
    }));
}

#[test]
fn replay_outbox_recovers_after_restart_for_queued_ingest() {
    let temp = tempdir().expect("tempdir");
    let src = temp.path().join("restart_queued.txt");
    fs::write(&src, "OAuth restart queue recovery").expect("write queued");

    let app1 = AxiomMe::new(temp.path()).expect("app1 new");
    app1.initialize().expect("app1 init failed");
    let add = app1
        .add_resource(
            src.to_str().expect("src str"),
            Some("axiom://resources/restart-queued"),
            None,
            None,
            false,
            None,
        )
        .expect("add failed");
    assert!(add.queued);
    let pending = app1.state.fetch_outbox("new", 50).expect("pending events");
    assert!(pending.iter().any(|event| {
        event.event_type == "semantic_scan" && event.uri == "axiom://resources/restart-queued"
    }));

    let before_restart = app1.queue_diagnostics().expect("queue before restart");
    assert!(before_restart.counts.new_total >= 1);
    drop(app1);

    let app2 = AxiomMe::new(temp.path()).expect("app2 new");
    let before_replay = app2
        .find(
            "oauth",
            Some("axiom://resources/restart-queued"),
            Some(5),
            None,
            None,
        )
        .expect("find before replay");
    assert!(before_replay.query_results.is_empty());

    let replay = app2.replay_outbox(100, false).expect("replay failed");
    assert!(replay.processed >= 1);
    let done = app2.state.fetch_outbox("done", 200).expect("done outbox");
    assert!(done.iter().any(|event| {
        event.event_type == "semantic_scan" && event.uri == "axiom://resources/restart-queued"
    }));

    let after_replay = app2
        .find(
            "oauth",
            Some("axiom://resources/restart-queued"),
            Some(5),
            None,
            None,
        )
        .expect("find after replay");
    assert!(!after_replay.query_results.is_empty());
}

#[test]
fn ingest_failure_missing_source_cleans_temp_and_logs_error() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let missing = temp.path().join("missing-ingest-source");
    let err = app
        .add_resource(
            missing.to_str().expect("missing path"),
            Some("axiom://resources/ingest-fail"),
            None,
            None,
            true,
            None,
        )
        .expect_err("must fail");
    assert!(matches!(err, AxiomError::NotFound(_)));

    let temp_root = app
        .fs
        .resolve_uri(&AxiomUri::parse("axiom://temp/ingest").expect("temp uri"));
    let entries = fs::read_dir(&temp_root).expect("read temp ingest");
    assert_eq!(entries.count(), 0);

    let target = AxiomUri::parse("axiom://resources/ingest-fail").expect("target");
    assert!(!app.fs.exists(&target));

    let logs = app
        .list_request_logs_filtered(20, Some("add_resource"), Some("error"))
        .expect("logs");
    assert!(
        logs.iter()
            .any(|entry| entry.error_code.as_deref() == Some("NOT_FOUND"))
    );
}

#[test]
fn tier_generation_is_deterministic_and_sorted() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let corpus = temp.path().join("tier_det_corpus");
    fs::create_dir_all(corpus.join("b-dir")).expect("mkdir corpus");
    fs::write(corpus.join("z-last.txt"), "tail entry").expect("write z");
    fs::write(corpus.join("a-first.txt"), "head entry").expect("write a");
    fs::write(corpus.join("b-dir/nested.md"), "nested entry").expect("write nested");

    let target = "axiom://resources/tier-det";
    app.add_resource(
        corpus.to_str().expect("corpus"),
        Some(target),
        None,
        None,
        true,
        None,
    )
    .expect("first add");

    let abstract_first = app.abstract_text(target).expect("abstract first");
    let overview_first = app.overview(target).expect("overview first");
    assert_eq!(
        abstract_first,
        "axiom://resources/tier-det contains 3 items"
    );

    let listed = overview_first
        .lines()
        .filter_map(|line| line.strip_prefix("- "))
        .collect::<Vec<_>>();
    assert_eq!(listed, vec!["a-first.txt", "b-dir", "z-last.txt"]);

    app.add_resource(
        corpus.to_str().expect("corpus"),
        Some(target),
        None,
        None,
        true,
        None,
    )
    .expect("second add");

    let abstract_second = app.abstract_text(target).expect("abstract second");
    let overview_second = app.overview(target).expect("overview second");
    assert_eq!(abstract_first, abstract_second);
    assert_eq!(overview_first, overview_second);

    let done = app.state.fetch_outbox("done", 400).expect("done outbox");
    assert!(done.iter().any(|event| {
        event.event_type == "upsert"
            && event.uri == target
            && event
                .payload_json
                .get("kind")
                .and_then(|value| value.as_str())
                == Some("dir")
    }));
}

#[test]
fn tier_generation_handles_empty_directory_and_observability() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let empty = temp.path().join("tier_empty_corpus");
    fs::create_dir_all(&empty).expect("mkdir empty corpus");
    let target = "axiom://resources/tier-empty";
    app.add_resource(
        empty.to_str().expect("empty"),
        Some(target),
        None,
        None,
        true,
        None,
    )
    .expect("add empty");

    let abstract_text = app.abstract_text(target).expect("empty abstract");
    let overview = app.overview(target).expect("empty overview");
    assert_eq!(
        abstract_text,
        "axiom://resources/tier-empty contains 0 items"
    );
    assert!(overview.contains("(empty)"));
    assert!(!overview.lines().any(|line| line.starts_with("- ")));

    let done = app.state.fetch_outbox("done", 300).expect("done outbox");
    assert!(done.iter().any(|event| {
        event.event_type == "upsert"
            && event.uri == target
            && event
                .payload_json
                .get("kind")
                .and_then(|value| value.as_str())
                == Some("dir")
    }));
}

#[test]
fn tier_generation_recovers_missing_artifact_after_drift_reindex() {
    let temp = tempdir().expect("tempdir");
    let corpus = temp.path().join("tier_drift_corpus");
    fs::create_dir_all(&corpus).expect("mkdir corpus");
    fs::write(corpus.join("auth.md"), "OAuth drift recovery").expect("write source");

    let target = "axiom://resources/tier-drift";
    let app1 = AxiomMe::new(temp.path()).expect("app1 new");
    app1.initialize().expect("app1 init");
    app1.add_resource(
        corpus.to_str().expect("corpus"),
        Some(target),
        None,
        None,
        true,
        None,
    )
    .expect("add");

    let initial_overview = app1.overview(target).expect("initial overview");
    assert!(initial_overview.contains("auth.md"));

    let overview_path = temp.path().join("resources/tier-drift/.overview.md");
    fs::remove_file(&overview_path).expect("remove overview artifact");
    drop(app1);

    let app2 = AxiomMe::new(temp.path()).expect("app2 new");
    app2.initialize().expect("app2 init");
    let restored = app2.overview(target).expect("restored overview");
    assert!(restored.contains("auth.md"));

    let find = app2
        .find("drift", Some(target), Some(5), None, None)
        .expect("find after drift");
    assert!(!find.query_results.is_empty());
}

#[test]
fn initialize_forces_reindex_when_profile_stamp_changes() {
    let temp = tempdir().expect("tempdir");
    let src = temp.path().join("stamp_policy.txt");
    fs::write(&src, "OAuth forced reindex policy").expect("write source");

    let app1 = AxiomMe::new(temp.path()).expect("app1 new");
    app1.initialize().expect("app1 init failed");
    app1.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/reindex-policy"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let before = app1
        .find(
            "oauth",
            Some("axiom://resources/reindex-policy"),
            Some(5),
            None,
            None,
        )
        .expect("find before");
    assert!(!before.query_results.is_empty());

    app1.state
        .set_system_value("index_profile_stamp", "outdated-stamp")
        .expect("set outdated stamp");
    drop(app1);

    fs::remove_dir_all(temp.path().join("resources/reindex-policy")).expect("remove indexed tree");

    let app2 = AxiomMe::new(temp.path()).expect("app2 new");
    app2.initialize().expect("app2 init failed");

    let after = app2
        .find(
            "oauth",
            Some("axiom://resources/reindex-policy"),
            Some(5),
            None,
            None,
        )
        .expect("find after");
    assert!(after.query_results.is_empty());

    let stamp = app2
        .state
        .get_system_value("index_profile_stamp")
        .expect("get stamp")
        .expect("missing stamp");
    assert_ne!(stamp, "outdated-stamp");
}

#[test]
fn initialize_reindexes_when_filesystem_drift_detected() {
    let temp = tempdir().expect("tempdir");
    let src = temp.path().join("drift_policy.txt");
    fs::write(&src, "OAuth old payload").expect("write source");

    let app1 = AxiomMe::new(temp.path()).expect("app1 new");
    app1.initialize().expect("app1 init failed");
    app1.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/drift-policy"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let entries = app1
        .ls("axiom://resources/drift-policy", true, false)
        .expect("ls");
    let leaf_uri = entries
        .iter()
        .find(|entry| !entry.is_dir && !entry.name.starts_with('.'))
        .map(|entry| entry.uri.clone())
        .expect("leaf uri");
    let leaf = AxiomUri::parse(&leaf_uri).expect("leaf parse");
    let leaf_path = app1.fs.resolve_uri(&leaf);
    fs::write(&leaf_path, "OAuth rotatedtoken payload").expect("rewrite indexed file");
    drop(app1);

    let app2 = AxiomMe::new(temp.path()).expect("app2 new");
    app2.initialize().expect("app2 init failed");
    let result = app2
        .find(
            "rotatedtoken",
            Some("axiom://resources/drift-policy"),
            Some(5),
            None,
            None,
        )
        .expect("find drift result");
    assert!(!result.query_results.is_empty());
}

#[test]
fn reconcile_prunes_missing_index_state() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    app.state
        .upsert_index_state("axiom://resources/ghost", "hash", 1, "indexed")
        .expect("upsert failed");

    let report = app.reconcile_state().expect("reconcile failed");
    assert!(report.drift_count >= 1);
    let hash = app
        .state
        .get_index_state_hash("axiom://resources/ghost")
        .expect("query failed");
    assert!(hash.is_none());
}

#[test]
fn replay_requeues_then_dead_letters_after_retry_budget() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let event_id = app
        .state
        .enqueue("semantic_scan", "invalid://uri", serde_json::json!({}))
        .expect("enqueue failed");

    let first = app.replay_outbox(10, false).expect("first replay");
    assert_eq!(first.fetched, 1);
    assert_eq!(first.requeued, 1);
    assert_eq!(first.dead_letter, 0);

    let new_events = app.state.fetch_outbox("new", 10).expect("fetch new");
    assert!(new_events.is_empty());
    let first_event = app
        .state
        .get_outbox_event(event_id)
        .expect("get event")
        .expect("missing event");
    assert_eq!(first_event.attempt_count, 1);
    assert_eq!(first_event.status, "new");

    for _ in 0..4 {
        app.state.force_outbox_due_now(event_id).expect("force due");
        let _ = app.replay_outbox(10, false).expect("replay loop");
    }

    let dead = app
        .state
        .fetch_outbox("dead_letter", 20)
        .expect("fetch dead");
    assert!(
        dead.iter()
            .any(|e| e.event_type == "semantic_scan" && e.uri == "invalid://uri")
    );
}

#[test]
fn retry_backoff_is_deterministic_and_bounded() {
    let a = retry_backoff_seconds("semantic_scan", 3, 101);
    let b = retry_backoff_seconds("semantic_scan", 3, 101);
    assert_eq!(a, b);
    assert!((4..=60).contains(&a));

    let c = retry_backoff_seconds("semantic_scan", 4, 101);
    assert!((8..=60).contains(&c));
}

#[test]
fn reconcile_dry_run_preserves_index_state() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    app.state
        .upsert_index_state("axiom://resources/ghost", "hash", 1, "indexed")
        .expect("upsert failed");

    let report = app
        .reconcile_state_with_options(ReconcileOptions {
            dry_run: true,
            scopes: Some(vec![Scope::Resources]),
            max_drift_sample: 10,
        })
        .expect("reconcile dry run");
    assert!(report.dry_run);
    assert!(report.drift_count >= 1);
    assert!(report.missing_files_pruned == 0);
    assert!(report.reindexed_scopes == 0);
    assert!(
        report
            .drift_uris_sample
            .iter()
            .any(|u| u == "axiom://resources/ghost")
    );

    let hash = app
        .state
        .get_index_state_hash("axiom://resources/ghost")
        .expect("query failed");
    assert_eq!(hash.as_deref(), Some("hash"));
}
