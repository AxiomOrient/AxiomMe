use super::*;

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
            "missing required bucket metric: {name}",
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
        .run_eval_loop_with_options(&EvalRunOptions {
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
fn eval_golden_merge_from_traces_skips_internal_or_root_expected_uri() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let traces_root = AxiomUri::root(Scope::Queue)
        .join("traces")
        .expect("traces root");

    let write_trace = |trace_id: &str, query: &str, expected_top_uri: &str| {
        let trace_uri = traces_root
            .join(&format!("{trace_id}.json"))
            .expect("trace uri");
        let trace = serde_json::json!({
            "trace_id": trace_id,
            "request_type": "search",
            "query": query,
            "target_uri": null,
            "start_points": [],
            "steps": [],
            "final_topk": [
                {"uri": expected_top_uri, "score": 1.0}
            ],
            "stop_reason": "completed",
            "metrics": {
                "latency_ms": 1,
                "explored_nodes": 1,
                "convergence_rounds": 1,
                "typed_query_count": 1,
                "relation_enriched_hits": 0,
                "relation_enriched_links": 0
            }
        });
        app.fs
            .write(
                &trace_uri,
                &serde_json::to_string_pretty(&trace).expect("serialize trace"),
                true,
            )
            .expect("write trace");
        app.state
            .upsert_trace_index(&TraceIndexEntry {
                trace_id: trace_id.to_string(),
                uri: trace_uri.to_string(),
                request_type: "search".to_string(),
                query: query.to_string(),
                target_uri: None,
                created_at: Utc::now().to_rfc3339(),
            })
            .expect("upsert trace index");
    };

    write_trace(
        "trace-valid-resource",
        "valid resource query",
        "axiom://resources/eval-merge-valid/doc.md",
    );
    write_trace(
        "trace-invalid-queue",
        "invalid queue query",
        "axiom://queue/logs/requests.jsonl",
    );
    write_trace(
        "trace-invalid-root",
        "invalid root query",
        "axiom://session",
    );

    let merge = app
        .merge_eval_golden_from_traces(50, 20)
        .expect("merge from traces");
    assert_eq!(merge.added_count, 1);

    let cases = app.list_eval_golden_queries().expect("list golden");
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].query, "valid resource query");
    assert_eq!(
        cases[0].expected_top_uri.as_deref(),
        Some("axiom://resources/eval-merge-valid/doc.md")
    );
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
fn eval_golden_loader_rejects_array_format() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let golden_uri = eval_golden_uri().expect("golden uri");
    app.fs
            .write(
                &golden_uri,
                r#"[{"source_trace_id":"array","query":"oauth","target_uri":"axiom://resources/demo","expected_top_uri":"axiom://resources/demo/file.md"}]"#,
                true,
            )
            .expect("write array payload");

    let err = app
        .list_eval_golden_queries()
        .expect_err("must reject array format");
    assert_eq!(err.code(), "JSON_ERROR");
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
        .run_eval_loop_with_options(&EvalRunOptions {
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
