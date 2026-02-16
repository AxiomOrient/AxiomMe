use super::*;

fn om_reflect_requested_payload(scope_key: &str, expected_generation: u32) -> serde_json::Value {
    serde_json::json!({
        "schema_version": crate::om_bridge::OM_OUTBOX_SCHEMA_VERSION_V1,
        "scope_key": scope_key,
        "expected_generation": expected_generation,
        "requested_at": chrono::Utc::now().to_rfc3339(),
    })
}

fn om_reflect_buffer_requested_payload(
    scope_key: &str,
    expected_generation: u32,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": crate::om_bridge::OM_OUTBOX_SCHEMA_VERSION_V1,
        "scope_key": scope_key,
        "expected_generation": expected_generation,
        "requested_at": chrono::Utc::now().to_rfc3339(),
    })
}

fn om_observe_buffer_requested_payload(
    scope_key: &str,
    expected_generation: u32,
    session_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": crate::om_bridge::OM_OUTBOX_SCHEMA_VERSION_V1,
        "scope_key": scope_key,
        "expected_generation": expected_generation,
        "requested_at": chrono::Utc::now().to_rfc3339(),
        "session_id": session_id,
    })
}

#[expect(
    clippy::too_many_lines,
    reason = "single end-to-end OM lifecycle narrative verifies ordering and idempotency together"
)]
fn run_scope_om_lifecycle_with_search_hint(
    scope: crate::om::OmScope,
    session_id: &str,
    thread_id: Option<&str>,
    resource_id: Option<&str>,
    expect_async_observe_outbox: bool,
) {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let scope_key = crate::om::build_scope_key(scope, Some(session_id), thread_id, resource_id)
        .expect("scope key");
    let session = match scope {
        crate::om::OmScope::Session => app.session(Some(session_id)),
        _ => app
            .session(Some(session_id))
            .with_om_scope(scope, thread_id, resource_id)
            .expect("scope"),
    };
    session.load().expect("session load");

    session
        .add_message("user", "x".repeat(26_000))
        .expect("append first");
    let first_new_events = app
        .state
        .fetch_outbox("new", 100)
        .expect("fetch first outbox");
    let maybe_observe_event = first_new_events
        .iter()
        .find(|event| {
            event.event_type == "om_observe_buffer_requested"
                && event
                    .payload_json
                    .get("scope_key")
                    .and_then(|value| value.as_str())
                    == Some(scope_key.as_str())
        })
        .cloned();

    if expect_async_observe_outbox {
        let observe_event = maybe_observe_event.expect("observe event");
        let expected_generation = observe_event
            .payload_json
            .get("expected_generation")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0);

        let replay_first = app.replay_outbox(100, false).expect("replay first");
        assert!(replay_first.done >= 1);

        let record_after_first = app
            .state
            .get_om_record_by_scope_key(&scope_key)
            .expect("om lookup after first replay")
            .expect("om record");
        let chunks_before = app
            .state
            .list_om_observation_chunks(&record_after_first.id)
            .expect("chunks before duplicate observe");
        assert!(!chunks_before.is_empty());

        session
            .process_om_observe_buffer_requested(&scope_key, expected_generation, observe_event.id)
            .expect("duplicate observe processing");
        let chunks_after = app
            .state
            .list_om_observation_chunks(&record_after_first.id)
            .expect("chunks after duplicate observe");
        assert_eq!(chunks_after.len(), chunks_before.len());
    } else {
        assert!(
            maybe_observe_event.is_none(),
            "resource scope should not enqueue async observe by default"
        );
    }

    for _ in 0..4 {
        session
            .add_message("user", "y".repeat(26_000))
            .expect("append follow-up");
    }
    let _ = app.replay_outbox(100, false).expect("replay auto events");

    let record_before_manual_reflect = app
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("lookup before manual reflect")
        .expect("om record");
    let generation_before_manual_reflect = record_before_manual_reflect.generation_count;

    let reflect_uri = format!("axiom://session/{session_id}");
    let apply_event_id = app
        .state
        .enqueue(
            "om_reflect_requested",
            &reflect_uri,
            om_reflect_requested_payload(&scope_key, generation_before_manual_reflect),
        )
        .expect("enqueue manual apply");
    let stale_event_id = app
        .state
        .enqueue(
            "om_reflect_requested",
            &reflect_uri,
            om_reflect_requested_payload(&scope_key, generation_before_manual_reflect),
        )
        .expect("enqueue manual stale");

    let replay_manual = app.replay_outbox(100, false).expect("replay manual");
    assert!(replay_manual.done >= 2);
    assert_eq!(replay_manual.dead_letter, 0);

    let final_record = app
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("final om lookup")
        .expect("final om record");
    assert_eq!(
        final_record.generation_count,
        generation_before_manual_reflect.saturating_add(1)
    );
    assert_eq!(
        final_record.last_applied_outbox_event_id,
        Some(apply_event_id)
    );
    assert!(
        !final_record.active_observations.trim().is_empty()
            || final_record
                .suggested_response
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
    );

    let search = app
        .search(
            "scope lifecycle",
            None,
            Some(session_id),
            Some(5),
            None,
            None,
        )
        .expect("search");
    let notes = search.query_plan["notes"].as_array().expect("search notes");
    assert!(
        notes
            .iter()
            .filter_map(|value| value.as_str())
            .any(|value| value.starts_with("om_hint_applied:1"))
    );

    let logs = app
        .list_request_logs_filtered(10, Some("search"), Some("ok"))
        .expect("request logs");
    let details = logs
        .first()
        .and_then(|entry| entry.details.as_ref())
        .expect("search details");
    assert_eq!(
        details
            .get("om_hint_applied")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );

    let done = app.state.fetch_outbox("done", 300).expect("done outbox");
    assert!(done.iter().any(|event| event.id == apply_event_id));
    assert!(done.iter().any(|event| event.id == stale_event_id));
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
#[expect(
    clippy::too_many_lines,
    reason = "scenario compares wait and replay behavior across full queue lifecycle"
)]
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
fn queue_overview_matches_diagnostics_and_lane_snapshot() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let overview = app.queue_overview().expect("overview");
    let diagnostics = app.queue_diagnostics().expect("diagnostics");
    let lanes = app.state.queue_status().expect("lane snapshot");

    assert_eq!(overview.counts.new_total, diagnostics.counts.new_total);
    assert_eq!(overview.counts.new_due, diagnostics.counts.new_due);
    assert_eq!(overview.counts.processing, diagnostics.counts.processing);
    assert_eq!(overview.counts.done, diagnostics.counts.done);
    assert_eq!(overview.counts.dead_letter, diagnostics.counts.dead_letter);
    assert_eq!(
        overview.counts.earliest_next_attempt_at,
        diagnostics.counts.earliest_next_attempt_at
    );
    assert_eq!(overview.checkpoints.len(), diagnostics.checkpoints.len());
    assert_eq!(
        overview.queue_dead_letter_rate,
        diagnostics.queue_dead_letter_rate
    );
    assert_eq!(overview.om_status, diagnostics.om_status);
    assert_eq!(
        overview.om_reflection_apply_metrics,
        diagnostics.om_reflection_apply_metrics
    );

    assert_eq!(overview.lanes.semantic.processed, lanes.semantic.processed);
    assert_eq!(
        overview.lanes.semantic.error_count,
        lanes.semantic.error_count
    );
    assert_eq!(
        overview.lanes.embedding.processed,
        lanes.embedding.processed
    );
    assert_eq!(
        overview.lanes.embedding.error_count,
        lanes.embedding.error_count
    );
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
fn replay_handles_om_reflection_event_with_cas_and_stale_noop() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let now = Utc::now();
    let scope_key = "session:s-om-reflect".to_string();
    app.state
        .upsert_om_record(&crate::om::OmRecord {
            id: "om-reflect-record".to_string(),
            scope: crate::om::OmScope::Session,
            scope_key: scope_key.clone(),
            session_id: Some("s-om-reflect".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 0,
            last_applied_outbox_event_id: None,
            origin_type: crate::om::OmOriginType::Initial,
            active_observations: "obs-1\nobs-2\nobs-3".to_string(),
            observation_token_count: 90_000,
            pending_message_tokens: 0,
            last_observed_at: Some(now),
            current_task: None,
            suggested_response: None,
            last_activated_message_ids: Vec::new(),
            observer_trigger_count_total: 0,
            reflector_trigger_count_total: 0,
            is_observing: false,
            is_reflecting: true,
            is_buffering_observation: false,
            is_buffering_reflection: true,
            last_buffered_at_tokens: 0,
            last_buffered_at_time: None,
            buffered_reflection: None,
            buffered_reflection_tokens: None,
            buffered_reflection_input_tokens: None,
            reflected_observation_line_count: None,
            created_at: now,
            updated_at: now,
        })
        .expect("upsert om");

    let first_event_id = app
        .state
        .enqueue(
            "om_reflect_requested",
            "axiom://session/s-om-reflect",
            om_reflect_requested_payload(&scope_key, 0),
        )
        .expect("enqueue first");
    let second_event_id = app
        .state
        .enqueue(
            "om_reflect_requested",
            "axiom://session/s-om-reflect",
            om_reflect_requested_payload("session:s-om-reflect", 0),
        )
        .expect("enqueue second");

    let replay = app.replay_outbox(10, false).expect("replay");
    assert_eq!(replay.fetched, 2);
    assert_eq!(replay.done, 2);
    assert_eq!(replay.dead_letter, 0);
    assert_eq!(replay.requeued, 0);

    let record = app
        .state
        .get_om_record_by_scope_key("session:s-om-reflect")
        .expect("fetch om")
        .expect("om missing");
    assert_eq!(record.generation_count, 1);
    assert_eq!(record.last_applied_outbox_event_id, Some(first_event_id));
    assert!(record.buffered_reflection.is_none());
    assert!(record.buffered_reflection_tokens.is_none());
    assert!(record.buffered_reflection_input_tokens.is_none());
    assert_eq!(record.reflected_observation_line_count, Some(3));
    assert!(!record.is_reflecting);
    assert!(!record.is_buffering_reflection);
    assert!(
        !record.active_observations.is_empty(),
        "reflection must keep observations"
    );

    let done = app.state.fetch_outbox("done", 10).expect("done events");
    assert!(done.iter().any(|event| event.id == first_event_id));
    assert!(done.iter().any(|event| event.id == second_event_id));
    let dead = app
        .state
        .fetch_outbox("dead_letter", 10)
        .expect("dead-letter events");
    assert!(!dead.iter().any(|event| event.id == first_event_id));
    assert!(!dead.iter().any(|event| event.id == second_event_id));
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "scenario asserts buffered reflection replay, apply, and stale no-op in one timeline"
)]
fn replay_handles_om_reflection_buffer_then_apply_with_stale_noop() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let now = Utc::now();
    let scope_key = "session:s-om-reflect-buffer".to_string();
    app.state
        .upsert_om_record(&crate::om::OmRecord {
            id: "om-reflect-buffer-record".to_string(),
            scope: crate::om::OmScope::Session,
            scope_key: scope_key.clone(),
            session_id: Some("s-om-reflect-buffer".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 0,
            last_applied_outbox_event_id: None,
            origin_type: crate::om::OmOriginType::Initial,
            active_observations: "obs-1\nobs-2\nobs-3".to_string(),
            observation_token_count: 45_000,
            pending_message_tokens: 0,
            last_observed_at: Some(now),
            current_task: None,
            suggested_response: None,
            last_activated_message_ids: Vec::new(),
            observer_trigger_count_total: 0,
            reflector_trigger_count_total: 0,
            is_observing: false,
            is_reflecting: false,
            is_buffering_observation: false,
            is_buffering_reflection: true,
            last_buffered_at_tokens: 0,
            last_buffered_at_time: None,
            buffered_reflection: None,
            buffered_reflection_tokens: None,
            buffered_reflection_input_tokens: None,
            reflected_observation_line_count: None,
            created_at: now,
            updated_at: now,
        })
        .expect("upsert om");

    let buffer_event_1 = app
        .state
        .enqueue(
            "om_reflect_buffer_requested",
            "axiom://session/s-om-reflect-buffer",
            om_reflect_buffer_requested_payload(&scope_key, 0),
        )
        .expect("enqueue buffer 1");
    let buffer_event_2 = app
        .state
        .enqueue(
            "om_reflect_buffer_requested",
            "axiom://session/s-om-reflect-buffer",
            om_reflect_buffer_requested_payload("session:s-om-reflect-buffer", 0),
        )
        .expect("enqueue buffer 2");

    let replay_buffer = app.replay_outbox(10, false).expect("replay buffer");
    assert_eq!(replay_buffer.done, 2);
    assert_eq!(replay_buffer.dead_letter, 0);
    assert_eq!(replay_buffer.requeued, 0);

    let buffered_record = app
        .state
        .get_om_record_by_scope_key("session:s-om-reflect-buffer")
        .expect("fetch buffered record")
        .expect("record missing");
    assert_eq!(buffered_record.generation_count, 0);
    assert!(buffered_record.buffered_reflection.is_some());
    assert!(buffered_record.buffered_reflection_tokens.is_some());
    assert!(buffered_record.buffered_reflection_input_tokens.is_some());
    assert!(!buffered_record.is_buffering_reflection);

    let apply_event = app
        .state
        .enqueue(
            "om_reflect_requested",
            "axiom://session/s-om-reflect-buffer",
            om_reflect_requested_payload("session:s-om-reflect-buffer", 0),
        )
        .expect("enqueue apply");
    let stale_event = app
        .state
        .enqueue(
            "om_reflect_requested",
            "axiom://session/s-om-reflect-buffer",
            om_reflect_requested_payload("session:s-om-reflect-buffer", 0),
        )
        .expect("enqueue stale");

    let replay_apply = app.replay_outbox(10, false).expect("replay apply");
    assert_eq!(replay_apply.done, 2);
    assert_eq!(replay_apply.dead_letter, 0);
    assert_eq!(replay_apply.requeued, 0);

    let final_record = app
        .state
        .get_om_record_by_scope_key("session:s-om-reflect-buffer")
        .expect("fetch final record")
        .expect("record missing");
    assert_eq!(final_record.generation_count, 1);
    assert_eq!(final_record.last_applied_outbox_event_id, Some(apply_event));
    assert!(final_record.buffered_reflection.is_none());
    assert!(final_record.buffered_reflection_tokens.is_none());
    assert!(final_record.buffered_reflection_input_tokens.is_none());
    assert!(!final_record.is_reflecting);
    assert!(!final_record.is_buffering_reflection);

    let done = app.state.fetch_outbox("done", 10).expect("done events");
    assert!(done.iter().any(|event| event.id == buffer_event_1));
    assert!(done.iter().any(|event| event.id == buffer_event_2));
    assert!(done.iter().any(|event| event.id == apply_event));
    assert!(done.iter().any(|event| event.id == stale_event));
    let dead = app
        .state
        .fetch_outbox("dead_letter", 10)
        .expect("dead-letter events");
    assert!(!dead.iter().any(|event| {
        event.id == buffer_event_1
            || event.id == buffer_event_2
            || event.id == apply_event
            || event.id == stale_event
    }));
}

#[test]
fn replay_dead_letters_malformed_om_reflect_requested_immediately_on_schema_mismatch() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let event_id = app
        .state
        .enqueue(
            "om_reflect_requested",
            "axiom://session/s-om-malformed-reflect",
            serde_json::json!({
                "expected_generation": 0
            }),
        )
        .expect("enqueue malformed event");

    let replay = app.replay_outbox(10, false).expect("replay");
    assert_eq!(replay.dead_letter, 1);
    assert_eq!(replay.requeued, 0);

    let event = app
        .state
        .get_outbox_event(event_id)
        .expect("event lookup")
        .expect("event missing");
    assert_eq!(event.status, "dead_letter");
    assert_eq!(event.attempt_count, 1);

    let dead = app
        .state
        .fetch_outbox("dead_letter", 20)
        .expect("dead-letter events");
    assert!(dead.iter().any(|item| item.id == event_id));
}

#[test]
fn replay_dead_lettered_om_reflect_requested_clears_reflection_flags_when_scope_payload_is_usable()
{
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let now = Utc::now();
    let scope_key = "session:s-om-deadletter-reflect-clear";
    app.state
        .upsert_om_record(&crate::om::OmRecord {
            id: "om-deadletter-reflect-clear".to_string(),
            scope: crate::om::OmScope::Session,
            scope_key: scope_key.to_string(),
            session_id: Some("s-om-deadletter-reflect-clear".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 0,
            last_applied_outbox_event_id: None,
            origin_type: crate::om::OmOriginType::Initial,
            active_observations: "obs-1".to_string(),
            observation_token_count: 10_000,
            pending_message_tokens: 0,
            last_observed_at: Some(now),
            current_task: None,
            suggested_response: None,
            last_activated_message_ids: Vec::new(),
            observer_trigger_count_total: 0,
            reflector_trigger_count_total: 0,
            is_observing: false,
            is_reflecting: true,
            is_buffering_observation: false,
            is_buffering_reflection: true,
            last_buffered_at_tokens: 0,
            last_buffered_at_time: None,
            buffered_reflection: None,
            buffered_reflection_tokens: None,
            buffered_reflection_input_tokens: None,
            reflected_observation_line_count: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed om record");

    let event_id = app
        .state
        .enqueue(
            "om_reflect_requested",
            "axiom://session/s-om-deadletter-reflect-clear",
            serde_json::json!({
                "schema_version": 99,
                "scope_key": scope_key,
                "expected_generation": 0,
                "requested_at": Utc::now().to_rfc3339(),
            }),
        )
        .expect("enqueue malformed event");

    let replay = app.replay_outbox(10, false).expect("replay");
    assert_eq!(replay.dead_letter, 1);
    assert_eq!(replay.requeued, 0);

    let record = app
        .state
        .get_om_record_by_scope_key(scope_key)
        .expect("record lookup")
        .expect("record missing");
    assert_eq!(record.generation_count, 0);
    assert!(!record.is_reflecting);
    assert!(!record.is_buffering_reflection);

    let event = app
        .state
        .get_outbox_event(event_id)
        .expect("event lookup")
        .expect("event missing");
    assert_eq!(event.status, "dead_letter");
}

#[test]
fn replay_dead_letters_malformed_om_reflect_buffer_requested_immediately_on_schema_mismatch() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let event_id = app
        .state
        .enqueue(
            "om_reflect_buffer_requested",
            "axiom://session/s-om-malformed-buffer",
            serde_json::json!({
                "scope_key": "session:s-om-malformed-buffer",
                "expected_generation": "zero"
            }),
        )
        .expect("enqueue malformed event");

    let replay = app.replay_outbox(10, false).expect("replay");
    assert_eq!(replay.dead_letter, 1);
    assert_eq!(replay.requeued, 0);

    let event = app
        .state
        .get_outbox_event(event_id)
        .expect("event lookup")
        .expect("event missing");
    assert_eq!(event.status, "dead_letter");
    assert_eq!(event.attempt_count, 1);

    let dead = app
        .state
        .fetch_outbox("dead_letter", 20)
        .expect("dead-letter events");
    assert!(dead.iter().any(|item| item.id == event_id));
}

#[test]
fn replay_dead_lettered_om_reflect_buffer_requested_clears_reflection_flags_when_scope_payload_is_usable()
 {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let now = Utc::now();
    let scope_key = "session:s-om-deadletter-reflect-buffer-clear";
    app.state
        .upsert_om_record(&crate::om::OmRecord {
            id: "om-deadletter-reflect-buffer-clear".to_string(),
            scope: crate::om::OmScope::Session,
            scope_key: scope_key.to_string(),
            session_id: Some("s-om-deadletter-reflect-buffer-clear".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 0,
            last_applied_outbox_event_id: None,
            origin_type: crate::om::OmOriginType::Initial,
            active_observations: "obs-1".to_string(),
            observation_token_count: 10_000,
            pending_message_tokens: 0,
            last_observed_at: Some(now),
            current_task: None,
            suggested_response: None,
            last_activated_message_ids: Vec::new(),
            observer_trigger_count_total: 0,
            reflector_trigger_count_total: 0,
            is_observing: false,
            is_reflecting: false,
            is_buffering_observation: false,
            is_buffering_reflection: true,
            last_buffered_at_tokens: 0,
            last_buffered_at_time: None,
            buffered_reflection: None,
            buffered_reflection_tokens: None,
            buffered_reflection_input_tokens: None,
            reflected_observation_line_count: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed om record");

    let event_id = app
        .state
        .enqueue(
            "om_reflect_buffer_requested",
            "axiom://session/s-om-deadletter-reflect-buffer-clear",
            serde_json::json!({
                "schema_version": 99,
                "scope_key": scope_key,
                "expected_generation": 0,
                "requested_at": Utc::now().to_rfc3339(),
            }),
        )
        .expect("enqueue malformed event");

    let replay = app.replay_outbox(10, false).expect("replay");
    assert_eq!(replay.dead_letter, 1);
    assert_eq!(replay.requeued, 0);

    let record = app
        .state
        .get_om_record_by_scope_key(scope_key)
        .expect("record lookup")
        .expect("record missing");
    assert!(!record.is_reflecting);
    assert!(!record.is_buffering_reflection);

    let event = app
        .state
        .get_outbox_event(event_id)
        .expect("event lookup")
        .expect("event missing");
    assert_eq!(event.status, "dead_letter");
}

#[test]
fn replay_dead_lettered_om_reflect_requested_does_not_clear_flags_when_generation_mismatches() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let now = Utc::now();
    let scope_key = "session:s-om-deadletter-reflect-generation-mismatch";
    app.state
        .upsert_om_record(&crate::om::OmRecord {
            id: "om-deadletter-reflect-generation-mismatch".to_string(),
            scope: crate::om::OmScope::Session,
            scope_key: scope_key.to_string(),
            session_id: Some("s-om-deadletter-reflect-generation-mismatch".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 1,
            last_applied_outbox_event_id: None,
            origin_type: crate::om::OmOriginType::Initial,
            active_observations: "obs-1".to_string(),
            observation_token_count: 10_000,
            pending_message_tokens: 0,
            last_observed_at: Some(now),
            current_task: None,
            suggested_response: None,
            last_activated_message_ids: Vec::new(),
            observer_trigger_count_total: 0,
            reflector_trigger_count_total: 0,
            is_observing: false,
            is_reflecting: true,
            is_buffering_observation: false,
            is_buffering_reflection: true,
            last_buffered_at_tokens: 0,
            last_buffered_at_time: None,
            buffered_reflection: None,
            buffered_reflection_tokens: None,
            buffered_reflection_input_tokens: None,
            reflected_observation_line_count: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed om record");

    let _event_id = app
        .state
        .enqueue(
            "om_reflect_requested",
            "axiom://session/s-om-deadletter-reflect-generation-mismatch",
            serde_json::json!({
                "schema_version": 99,
                "scope_key": scope_key,
                "expected_generation": 0,
                "requested_at": Utc::now().to_rfc3339(),
            }),
        )
        .expect("enqueue malformed event");

    let replay = app.replay_outbox(10, false).expect("replay");
    assert_eq!(replay.dead_letter, 1);
    assert_eq!(replay.requeued, 0);

    let record = app
        .state
        .get_om_record_by_scope_key(scope_key)
        .expect("record lookup")
        .expect("record missing");
    assert!(record.is_reflecting);
    assert!(record.is_buffering_reflection);
}

#[test]
fn replay_handles_om_observe_buffer_requested_event() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let session = app.session(Some("s-om-observe-replay"));
    session.load().expect("session load");
    session
        .add_message("user", "x".repeat(26_000))
        .expect("append");

    let scope_key = crate::om::build_scope_key(
        crate::om::OmScope::Session,
        Some("s-om-observe-replay"),
        None,
        None,
    )
    .expect("scope key");
    let queued = app.state.fetch_outbox("new", 20).expect("fetch outbox");
    let observe_event = queued
        .iter()
        .find(|event| event.event_type == "om_observe_buffer_requested")
        .expect("om_observe_buffer_requested event not queued");
    assert_eq!(
        observe_event
            .payload_json
            .get("schema_version")
            .and_then(serde_json::Value::as_u64),
        Some(u64::from(crate::om_bridge::OM_OUTBOX_SCHEMA_VERSION_V1))
    );
    assert_eq!(
        observe_event
            .payload_json
            .get("scope_key")
            .and_then(|value| value.as_str()),
        Some(scope_key.as_str())
    );
    assert!(
        observe_event
            .payload_json
            .get("requested_at")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
    );
    assert_eq!(
        observe_event
            .payload_json
            .get("expected_generation")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );

    let replay = app.replay_outbox(20, false).expect("replay");
    assert!(replay.done >= 1);
    assert_eq!(replay.dead_letter, 0);

    let record = app
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("fetch om")
        .expect("om record missing");
    let chunks = app
        .state
        .list_om_observation_chunks(&record.id)
        .expect("fetch chunks");
    assert_eq!(record.observer_trigger_count_total, 1);
    assert_eq!(chunks.len(), 1);
    assert!(record.is_buffering_observation);
    assert!(!chunks[0].observations.trim().is_empty());
}

#[test]
fn replay_retries_observe_event_after_transient_payload_failure_without_duplicate_append() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let session = app.session(Some("s-om-observe-retry"));
    session.load().expect("session load");
    session
        .add_message("user", "x".repeat(26_000))
        .expect("append");

    let scope_key = crate::om::build_scope_key(
        crate::om::OmScope::Session,
        Some("s-om-observe-retry"),
        None,
        None,
    )
    .expect("scope key");
    let queued = app.state.fetch_outbox("new", 20).expect("fetch outbox");
    let observe_event = queued
        .iter()
        .find(|event| event.event_type == "om_observe_buffer_requested")
        .expect("observe event");

    app.state
        .update_outbox_payload_json(
            observe_event.id,
            &om_observe_buffer_requested_payload(&scope_key, 0, "../bad-session-id"),
        )
        .expect("corrupt observe payload");

    let first = app.replay_outbox(20, false).expect("first replay");
    assert_eq!(first.fetched, 1);
    assert_eq!(first.done, 0);
    assert_eq!(first.requeued, 1);
    assert_eq!(first.dead_letter, 0);

    let retriable = app
        .state
        .get_outbox_event(observe_event.id)
        .expect("event lookup")
        .expect("event missing");
    assert_eq!(retriable.status, "new");
    assert_eq!(retriable.attempt_count, 1);

    app.state
        .update_outbox_payload_json(
            observe_event.id,
            &om_observe_buffer_requested_payload(
                "session:s-om-observe-retry",
                0,
                "s-om-observe-retry",
            ),
        )
        .expect("repair observe payload");

    app.state
        .force_outbox_due_now(observe_event.id)
        .expect("force due");
    let second = app.replay_outbox(20, false).expect("second replay");
    assert_eq!(second.fetched, 1);
    assert_eq!(second.done, 1);
    assert_eq!(second.requeued, 0);
    assert_eq!(second.dead_letter, 0);

    let record = app
        .state
        .get_om_record_by_scope_key("session:s-om-observe-retry")
        .expect("fetch om")
        .expect("om record missing");
    let chunks = app
        .state
        .list_om_observation_chunks(&record.id)
        .expect("fetch chunks");
    assert_eq!(chunks.len(), 1);
    assert!(
        app.state
            .om_observer_event_applied(observe_event.id)
            .expect("observe marker lookup"),
        "observe event should be marked as applied after successful retry"
    );
}

#[test]
fn replay_dead_letters_malformed_om_observe_buffer_requested_immediately_on_schema_mismatch() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let event_id = app
        .state
        .enqueue(
            "om_observe_buffer_requested",
            "axiom://session/s-om-malformed-observe",
            serde_json::json!({
                "scope_key": "session:s-om-malformed-observe",
                "expected_generation": "zero",
            }),
        )
        .expect("enqueue malformed event");

    let replay = app.replay_outbox(10, false).expect("replay");
    assert_eq!(replay.dead_letter, 1);
    assert_eq!(replay.requeued, 0);

    let event = app
        .state
        .get_outbox_event(event_id)
        .expect("event lookup")
        .expect("event missing");
    assert_eq!(event.status, "dead_letter");
    assert_eq!(event.attempt_count, 1);

    let dead = app
        .state
        .fetch_outbox("dead_letter", 20)
        .expect("dead-letter events");
    assert!(dead.iter().any(|item| item.id == event_id));
}

#[test]
fn scope_e2e_session_write_observe_reflect_search_hint() {
    run_scope_om_lifecycle_with_search_hint(
        crate::om::OmScope::Session,
        "s-om-scope-e2e-session",
        None,
        None,
        true,
    );
}

#[test]
fn scope_e2e_thread_write_observe_reflect_search_hint() {
    run_scope_om_lifecycle_with_search_hint(
        crate::om::OmScope::Thread,
        "s-om-scope-e2e-thread",
        Some("thread-scope-e2e"),
        Some("resource-scope-e2e"),
        true,
    );
}

#[test]
fn scope_e2e_resource_write_reflect_search_hint() {
    run_scope_om_lifecycle_with_search_hint(
        crate::om::OmScope::Resource,
        "s-om-scope-e2e-resource",
        Some("thread-scope-e2e-resource"),
        Some("resource-scope-e2e-resource"),
        false,
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
        .reconcile_state_with_options(&ReconcileOptions {
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
