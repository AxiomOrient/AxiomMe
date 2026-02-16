use super::*;

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
    let probe_uri = AxiomUri::parse(&report.queued_root_uri).expect("probe uri parse");
    assert!(
        !app.fs.exists(&probe_uri),
        "reliability probe data must be cleaned up after report generation"
    );
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
fn release_gate_pack_fails_fast_for_missing_workspace_path() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let options = crate::models::ReleaseGatePackOptions {
        workspace_dir: Some(temp.path().join("missing-workspace").display().to_string()),
        ..crate::models::ReleaseGatePackOptions::default()
    };
    let err = app
        .collect_release_gate_pack(&options)
        .expect_err("missing workspace must fail");
    assert!(matches!(err, AxiomError::NotFound(_)));
}

#[test]
fn release_gate_pack_fails_fast_for_workspace_without_manifest() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let workspace = temp.path().join("workspace-no-manifest");
    fs::create_dir_all(&workspace).expect("mkdir workspace");
    let options = crate::models::ReleaseGatePackOptions {
        workspace_dir: Some(workspace.display().to_string()),
        ..crate::models::ReleaseGatePackOptions::default()
    };
    let err = app
        .collect_release_gate_pack(&options)
        .expect_err("workspace without Cargo.toml must fail");
    assert!(matches!(err, AxiomError::Validation(_)));
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "release pack happy path validates full gate orchestration in one deterministic scenario"
)]
fn release_gate_pack_happy_path_runs_with_mocked_workspace_commands() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let workspace = temp.path().join("workspace-release-pack");
    fs::create_dir_all(workspace.join("crates").join("axiomme-core")).expect("mkdir core");

    fs::write(
        workspace.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/axiomme-core\"]\n",
    )
    .expect("write workspace Cargo.toml");
    fs::write(
        workspace
            .join("crates")
            .join("axiomme-core")
            .join("Cargo.toml"),
        "[package]\nname = \"axiomme-core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("write core Cargo.toml");
    let options = crate::models::ReleaseGatePackOptions {
        workspace_dir: Some(workspace.display().to_string()),
        replay_limit: 20,
        replay_max_cycles: 2,
        trace_limit: 20,
        request_limit: 20,
        eval_trace_limit: 10,
        eval_query_limit: 5,
        eval_search_limit: 5,
        benchmark_query_limit: 1,
        benchmark_search_limit: 5,
        benchmark_threshold_p95_ms: 10_000,
        benchmark_min_top1_accuracy: 0.0,
        benchmark_min_stress_top1_accuracy: None,
        benchmark_max_p95_regression_pct: None,
        benchmark_max_top1_regression_pct: None,
        benchmark_window_size: 1,
        benchmark_required_passes: 1,
        security_audit_mode: "offline".to_string(),
    };
    let report = with_workspace_command_mocks(
        &[
            ("cargo", &["check", "--workspace"], true, "check ok"),
            ("cargo", &["fmt", "--all", "--check"], true, "fmt ok"),
            (
                "cargo",
                &[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--",
                    "-D",
                    "warnings",
                ],
                true,
                "clippy ok",
            ),
            (
                "cargo",
                &[
                    "test",
                    "-p",
                    "axiomme-core",
                    "client::tests::relation_trace_logs::contract_execution_probe_validates_core_algorithms",
                    "--",
                    "--exact",
                ],
                true,
                "test client::tests::relation_trace_logs::contract_execution_probe_validates_core_algorithms ... ok",
            ),
        ],
        || app.collect_release_gate_pack(&options),
    )
    .expect("collect release gate pack");

    assert!(
        report
            .report_uri
            .starts_with("axiom://queue/release/packs/")
    );
    let report_uri = AxiomUri::parse(&report.report_uri).expect("report uri parse");
    assert!(app.fs.exists(&report_uri));

    for gate_id in ["G0", "G1", "G2", "G3", "G4", "G5", "G6", "G7", "G8"] {
        assert!(
            report
                .decisions
                .iter()
                .any(|decision| decision.gate_id == gate_id),
            "missing gate decision: {gate_id}"
        );
    }
    assert!(
        report
            .decisions
            .iter()
            .find(|decision| decision.gate_id == "G0")
            .expect("G0 decision")
            .passed,
        "G0 should pass when contract execution probe is mocked to succeed"
    );
    assert!(
        report
            .decisions
            .iter()
            .find(|decision| decision.gate_id == "G1")
            .expect("G1 decision")
            .passed,
        "G1 should pass when workspace build commands are mocked to succeed"
    );
}

#[test]
fn contract_integrity_gate_detects_missing_core_crate() {
    let temp = tempdir().expect("tempdir");

    let decision = evaluate_contract_integrity_gate(temp.path());
    assert_eq!(decision.gate_id, "G0");
    assert!(!decision.passed);
    assert!(decision.details.contains("missing_axiomme_core_crate"));
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
