use super::*;

fn write_ontology_schema(app: &AxiomMe, schema: &str) {
    let uri = AxiomUri::parse(crate::ontology::ONTOLOGY_SCHEMA_URI_V1).expect("schema uri parse");
    app.fs.write(&uri, schema, true).expect("write schema");
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
            Some(tag_filter),
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
fn episodic_api_probe_validates_om_contract() {
    let config = crate::om::resolve_om_config(crate::om::OmConfigInput::default())
        .expect("resolve om config");
    assert_eq!(config.scope, crate::om::OmScope::Thread);

    let scope_key =
        crate::om::build_scope_key(crate::om::OmScope::Thread, None, Some("thread-1"), None)
            .expect("build scope key");
    assert_eq!(scope_key, "thread:thread-1");

    let parsed = crate::om::parse_memory_section_xml(
        "<observations>\nalpha\n</observations>",
        crate::om::OmParseMode::Strict,
    );
    assert_eq!(parsed.observations.trim(), "alpha");
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "contract probe keeps multi-step algorithm verification in a single reproducible flow"
)]
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
    assert!(!search.query_plan.typed_queries.is_empty());
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

    let plan = result.query_plan;
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
fn relation_api_enforces_ontology_link_types_when_schema_exists() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    write_ontology_schema(
        &app,
        r#"{
            "version": 1,
            "object_types": [{
                "id": "resource_doc",
                "uri_prefixes": ["axiom://resources/relation-demo"],
                "allowed_scopes": ["resources"]
            }],
            "link_types": [{
                "id": "auth-security",
                "from_types": ["resource_doc"],
                "to_types": ["resource_doc"],
                "min_arity": 2,
                "max_arity": 4,
                "symmetric": true
            }],
            "action_types": [],
            "invariants": []
        }"#,
    );

    app.link(
        "axiom://resources/relation-demo",
        "auth-security",
        vec![
            "axiom://resources/relation-demo/auth".to_string(),
            "axiom://resources/relation-demo/security".to_string(),
        ],
        "typed relation",
    )
    .expect("declared link type should pass");

    let err = app
        .link(
            "axiom://resources/relation-demo",
            "undeclared-link",
            vec![
                "axiom://resources/relation-demo/auth".to_string(),
                "axiom://resources/relation-demo/security".to_string(),
            ],
            "must fail",
        )
        .expect_err("undeclared link type must fail");
    assert!(matches!(err, AxiomError::OntologyViolation(_)));
    assert!(err.to_string().contains("not declared"));
}

#[test]
fn relation_api_enforces_ontology_object_type_resolution_when_schema_exists() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    write_ontology_schema(
        &app,
        r#"{
            "version": 1,
            "object_types": [{
                "id": "resource_doc",
                "uri_prefixes": ["axiom://resources/relation-demo"],
                "allowed_scopes": ["resources"]
            }],
            "link_types": [{
                "id": "auth-security",
                "from_types": ["resource_doc"],
                "to_types": ["resource_doc"],
                "min_arity": 2,
                "max_arity": 4,
                "symmetric": true
            }],
            "action_types": [],
            "invariants": []
        }"#,
    );

    let err = app
        .link(
            "axiom://resources/relation-demo",
            "auth-security",
            vec![
                "axiom://resources/relation-demo/auth".to_string(),
                "axiom://user/notes/security".to_string(),
            ],
            "must fail",
        )
        .expect_err("endpoint type mismatch must fail");
    assert!(matches!(err, AxiomError::OntologyViolation(_)));
    assert!(err.to_string().contains("not resolved"));
}

#[test]
fn relation_api_refreshes_ontology_schema_cache_after_schema_update() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    write_ontology_schema(
        &app,
        r#"{
            "version": 1,
            "object_types": [{
                "id": "resource_doc",
                "uri_prefixes": ["axiom://resources/relation-demo"],
                "allowed_scopes": ["resources"]
            }],
            "link_types": [{
                "id": "auth-security",
                "from_types": ["resource_doc"],
                "to_types": ["resource_doc"],
                "min_arity": 2,
                "max_arity": 4,
                "symmetric": true
            }],
            "action_types": [],
            "invariants": []
        }"#,
    );
    app.link(
        "axiom://resources/relation-demo",
        "auth-security",
        vec![
            "axiom://resources/relation-demo/auth".to_string(),
            "axiom://resources/relation-demo/security".to_string(),
        ],
        "v1 link",
    )
    .expect("v1 link should pass");

    write_ontology_schema(
        &app,
        r#"{
            "version": 1,
            "object_types": [{
                "id": "resource_doc",
                "uri_prefixes": ["axiom://resources/relation-demo"],
                "allowed_scopes": ["resources"]
            }],
            "link_types": [{
                "id": "risk-review",
                "from_types": ["resource_doc"],
                "to_types": ["resource_doc"],
                "min_arity": 2,
                "max_arity": 8,
                "symmetric": false
            }],
            "action_types": [],
            "invariants": []
        }"#,
    );

    let stale = app
        .link(
            "axiom://resources/relation-demo",
            "auth-security",
            vec![
                "axiom://resources/relation-demo/auth".to_string(),
                "axiom://resources/relation-demo/security".to_string(),
            ],
            "old link id should fail after schema update",
        )
        .expect_err("old link id must fail");
    assert!(matches!(stale, AxiomError::OntologyViolation(_)));
    assert!(stale.to_string().contains("not declared"));

    app.link(
        "axiom://resources/relation-demo",
        "risk-review",
        vec![
            "axiom://resources/relation-demo/auth".to_string(),
            "axiom://resources/relation-demo/security".to_string(),
        ],
        "v2 link",
    )
    .expect("updated link should pass");
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
    assert!(find.query_results.iter().all(|hit| {
        hit.relations
            .iter()
            .all(|relation| relation.relation_type.is_none())
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
fn relation_enrichment_can_attach_typed_edge_metadata_when_enabled() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");
    write_ontology_schema(
        &app,
        r#"{
            "version": 1,
            "object_types": [{
                "id": "resource_doc",
                "uri_prefixes": ["axiom://resources/relation-typed-demo"],
                "allowed_scopes": ["resources"]
            }],
            "link_types": [{
                "id": "auth-security",
                "from_types": ["resource_doc"],
                "to_types": ["resource_doc"],
                "min_arity": 2,
                "max_arity": 4,
                "symmetric": true
            }],
            "action_types": [],
            "invariants": []
        }"#,
    );

    let corpus_dir = temp.path().join("relation_typed_corpus");
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
        Some("axiom://resources/relation-typed-demo"),
        None,
        None,
        true,
        None,
    )
    .expect("add");

    app.link(
        "axiom://resources/relation-typed-demo",
        "auth-security",
        vec![
            "axiom://resources/relation-typed-demo/auth.md".to_string(),
            "axiom://resources/relation-typed-demo/security.md".to_string(),
        ],
        "Security dependency",
    )
    .expect("link");

    let mut result = app
        .find(
            "oauth",
            Some("axiom://resources/relation-typed-demo"),
            Some(10),
            None,
            None,
        )
        .expect("find");
    app.enrich_find_result_relations(&mut result, 5, true)
        .expect("typed enrich");
    let relation = result
        .query_results
        .iter()
        .find(|hit| hit.uri.ends_with("auth.md"))
        .and_then(|hit| {
            hit.relations
                .iter()
                .find(|x| x.uri.ends_with("security.md"))
        })
        .expect("typed relation");
    assert_eq!(relation.relation_type.as_deref(), Some("auth-security"));
    assert_eq!(relation.source_object_type.as_deref(), Some("resource_doc"));
    assert_eq!(relation.target_object_type.as_deref(), Some("resource_doc"));
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
            .is_some_and(|trace| trace.request_type.ends_with("_replay"))
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
#[expect(
    clippy::too_many_lines,
    reason = "request-log contract test exercises multiple operations as one integrated scenario"
)]
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
        .run_eval_loop_with_options(&EvalRunOptions {
            trace_limit: 20,
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            golden_only: false,
        })
        .expect("eval");
    assert!(eval.executed_cases >= 1);

    let benchmark = app
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 20,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
            fixture_name: None,
        })
        .expect("benchmark");
    assert!(benchmark.executed_cases >= 1);

    let gate = app
        .benchmark_gate_with_options(BenchmarkGateOptions {
            gate_profile: "unit-test".to_string(),
            threshold_p95_ms: u128::MAX,
            min_top1_accuracy: 0.0,
            min_stress_top1_accuracy: None,
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
        .reconcile_state_with_options(&ReconcileOptions {
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
        "unexpected imported uri: {imported}",
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
            "missing request log for operation {operation}",
        );
    }

    let dry_run_logs = app
        .list_request_logs_filtered(20, Some("reconcile.run"), Some("dry_run"))
        .expect("filter reconcile");
    assert!(!dry_run_logs.is_empty());
}
