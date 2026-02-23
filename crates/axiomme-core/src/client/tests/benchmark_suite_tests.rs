use super::*;

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
    assert!(report.executed_cases >= 1);
    assert!(report.p95_latency_ms >= report.p50_latency_ms);
    let find_p95_us = report.p95_latency_us.expect("find p95 us");
    let find_p50_us = report.p50_latency_us.expect("find p50 us");
    assert!(find_p95_us >= find_p50_us);

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
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 20,
            search_limit: 10,
            include_golden: false,
            include_trace: true,
            include_stress: true,
            trace_expectations: true,
            fixture_name: None,
        })
        .expect("benchmark");

    assert!(!report.environment.machine_profile.trim().is_empty());
    assert!(!report.environment.cpu_model.trim().is_empty());
    assert!(!report.environment.os_version.trim().is_empty());
    assert!(!report.environment.rustc_version.trim().is_empty());
    assert_eq!(report.environment.retrieval_backend, "memory");
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
    assert!(
        report.p99_latency_us.expect("find p99 us") >= report.p95_latency_us.expect("find p95 us")
    );
    assert!(
        report.search_p99_latency_us.expect("search p99 us")
            >= report.search_p95_latency_us.expect("search p95 us")
    );
    assert!(
        report.commit_p99_latency_us.expect("commit p99 us")
            >= report.commit_p95_latency_us.expect("commit p95 us")
    );
}

#[test]
fn benchmark_trace_expectations_can_be_disabled() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_unlabeled_trace_input.txt");
    fs::write(&src, "OAuth unlabeled trace coverage.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-unlabeled-trace"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-unlabeled-trace"),
            Some(10),
            None,
            None,
        )
        .expect("find");

    let report = app
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 20,
            search_limit: 10,
            include_golden: false,
            include_trace: true,
            include_stress: false,
            trace_expectations: false,
            fixture_name: None,
        })
        .expect("benchmark");

    assert!(!report.results.is_empty());
    assert!(report.results.iter().all(|x| x.expected_top_uri.is_none()));
}

#[test]
fn benchmark_top1_accuracy_uses_only_graded_cases() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let golden_src = temp.path().join("bench_graded_input.txt");
    fs::write(&golden_src, "OAuth graded benchmark coverage.").expect("write golden input");
    app.add_resource(
        golden_src.to_str().expect("golden src str"),
        Some("axiom://resources/bench-graded"),
        None,
        None,
        true,
        None,
    )
    .expect("add golden failed");

    let unlabeled_src = temp.path().join("bench_ungraded_input.txt");
    fs::write(&unlabeled_src, "SQLite unlabeled benchmark coverage.").expect("write unlabeled");
    app.add_resource(
        unlabeled_src.to_str().expect("unlabeled src str"),
        Some("axiom://resources/bench-ungraded"),
        None,
        None,
        true,
        None,
    )
    .expect("add unlabeled failed");

    let graded_find = app
        .find(
            "oauth graded signal",
            Some("axiom://resources/bench-graded"),
            Some(10),
            None,
            None,
        )
        .expect("seed graded trace");
    let _ = app
        .find(
            "sqlite unlabeled signal",
            Some("axiom://resources/bench-ungraded"),
            Some(10),
            None,
            None,
        )
        .expect("seed unlabeled trace");

    app.add_eval_golden_query(
        "oauth graded signal",
        Some("axiom://resources/bench-graded"),
        graded_find
            .query_results
            .first()
            .map(|hit| hit.uri.as_str()),
    )
    .expect("golden add");

    let report = app
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 20,
            search_limit: 10,
            include_golden: true,
            include_trace: true,
            include_stress: false,
            trace_expectations: false,
            fixture_name: None,
        })
        .expect("benchmark");

    let graded_count = report
        .results
        .iter()
        .filter(|x| x.expected_top_uri.is_some())
        .count();
    let ungraded_count = report
        .results
        .iter()
        .filter(|x| x.expected_top_uri.is_none())
        .count();
    assert!(graded_count >= 1);
    assert!(ungraded_count >= 1);
    assert!((report.top1_accuracy - 1.0).abs() < f32::EPSILON);
}

#[test]
fn benchmark_top1_treats_duplicate_leaf_uri_as_equivalent() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("dup.rs");
    fs::write(
        &src,
        "URI equivalence benchmark duplicate leaf coverage marker.",
    )
    .expect("write input");
    let target_uri = "axiom://resources/bench-uri-equivalent/dup.rs";
    app.add_resource(
        src.to_str().expect("src str"),
        Some(target_uri),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");

    let query = "duplicate leaf uri benchmark marker";
    let seeded = app
        .find(query, Some(target_uri), Some(10), None, None)
        .expect("seed find");
    let seeded_top = seeded
        .query_results
        .first()
        .map(|hit| hit.uri.clone())
        .expect("seeded top uri");
    assert!(
        seeded_top.ends_with("/dup.rs/dup.rs"),
        "expected duplicate leaf uri shape, got: {seeded_top}"
    );

    app.add_eval_golden_query(query, Some(target_uri), Some(target_uri))
        .expect("add golden");

    let report = app
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 5,
            search_limit: 10,
            include_golden: true,
            include_trace: false,
            include_stress: false,
            trace_expectations: false,
            fixture_name: None,
        })
        .expect("benchmark");

    assert!(report.executed_cases >= 1);
    assert_eq!(report.passed, report.executed_cases);
    assert!((report.top1_accuracy - 1.0).abs() < f32::EPSILON);
}

#[test]
fn benchmark_stress_queries_are_included_from_golden_seed() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_stress_input.txt");
    fs::write(
        &src,
        "OAuth token refresh for stress query generation coverage.",
    )
    .expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-stress"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth token refresh",
            Some("axiom://resources/bench-stress"),
            Some(10),
            None,
            None,
        )
        .expect("find");
    app.add_eval_golden_query(
        "oauth token refresh",
        Some("axiom://resources/bench-stress"),
        Some("axiom://resources/bench-stress/bench_stress_input.txt"),
    )
    .expect("golden add");

    let report = app
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 20,
            search_limit: 10,
            include_golden: true,
            include_trace: false,
            include_stress: true,
            trace_expectations: false,
            fixture_name: None,
        })
        .expect("benchmark");

    assert!(
        report
            .results
            .iter()
            .any(|x| x.source.starts_with("stress:"))
    );
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
    app.add_eval_golden_query(
        "oauth",
        Some("axiom://resources/bench-rank"),
        Some("axiom://resources/bench-rank/bench_rank_input.txt"),
    )
    .expect("golden add");

    let report = app
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 20,
            search_limit: 10,
            include_golden: true,
            include_trace: false,
            include_stress: false,
            trace_expectations: false,
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
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: false,
            include_stress: true,
            trace_expectations: false,
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
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
            fixture_name: None,
        })
        .expect("bench 1");
    let _ = app
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
            fixture_name: None,
        })
        .expect("bench 2");

    let list = app.list_benchmark_reports(10).expect("list");
    assert!(list.len() >= 2);

    let trend = app.benchmark_trend(10).expect("trend");
    assert!(trend.latest.is_some());
    assert!(trend.previous.is_some());
    assert!(trend.delta_p95_latency_ms.is_some());
    assert!(trend.delta_p95_latency_us.is_some());
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
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
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
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
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
            min_stress_top1_accuracy: None,
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
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
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
            min_stress_top1_accuracy: None,
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
fn benchmark_gate_enforces_stress_top1_floor() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_stress_gate_input.txt");
    fs::write(&src, "OAuth benchmark stress gate content.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-stress-gate"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-stress-gate"),
            Some(5),
            None,
            None,
        )
        .expect("find");

    let template = app
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 12,
            search_limit: 5,
            include_golden: true,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
            fixture_name: None,
        })
        .expect("benchmark template");

    let mut latest = template;
    latest.run_id = "stress-floor-latest".to_string();
    latest.created_at = "2999-03-01T00:00:02Z".to_string();
    if let Some(first) = latest.results.first_mut() {
        first.source = "stress:synthetic".to_string();
        first.passed = false;
    }
    for result in &mut latest.results {
        if result.source.starts_with("stress:") {
            result.passed = false;
        }
    }

    let latest_uri = AxiomUri::parse("axiom://queue/benchmarks/reports/stress-floor-latest.json")
        .expect("latest");
    app.fs
        .write(
            &latest_uri,
            &serde_json::to_string_pretty(&latest).expect("serialize latest"),
            true,
        )
        .expect("write latest");

    let gate = app
        .benchmark_gate_with_options(BenchmarkGateOptions {
            gate_profile: "stress-floor-test".to_string(),
            threshold_p95_ms: 10_000,
            min_top1_accuracy: 0.0,
            min_stress_top1_accuracy: Some(0.9),
            max_p95_regression_pct: None,
            max_top1_regression_pct: None,
            window_size: 1,
            required_passes: 1,
            record: false,
            write_release_check: false,
        })
        .expect("gate");

    assert!(!gate.passed);
    assert!(gate.stress_top1_accuracy.is_some());
    assert!(gate.run_results[0].stress_top1_accuracy.is_some());
    assert!(
        gate.run_results[0]
            .reasons
            .iter()
            .any(|r| r.starts_with("stress_top1_accuracy_below:"))
    );
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
        .create_benchmark_fixture(
            "release-smoke",
            BenchmarkFixtureCreateOptions {
                query_limit: 10,
                include_golden: false,
                include_trace: true,
                include_stress: true,
                trace_expectations: false,
            },
        )
        .expect("create fixture");
    assert!(fixture.case_count >= 1);

    let fixtures = app.list_benchmark_fixtures(20).expect("list fixtures");
    assert!(fixtures.iter().any(|f| f.name == "release-smoke"));

    let report = app
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: false,
            include_stress: true,
            trace_expectations: false,
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
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
            fixture_name: None,
        })
        .expect("bench 1");
    let _ = app
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
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
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
            fixture_name: None,
        })
        .expect("bench 1");
    let _ = app
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
            fixture_name: None,
        })
        .expect("bench 2");

    let gate = app
        .benchmark_gate_with_options(BenchmarkGateOptions {
            gate_profile: "macmini-release".to_string(),
            threshold_p95_ms: 10_000,
            min_top1_accuracy: 0.0,
            min_stress_top1_accuracy: None,
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
    assert!(!gate.passed);
    assert_eq!(gate.embedding_provider.as_deref(), Some("semantic-lite"));
    assert!(gate.embedding_strict_error.is_none());
    assert!(
        gate.reasons
            .iter()
            .any(|reason| { reason.starts_with("release_embedding_provider_required:") })
    );

    let release_uri = AxiomUri::parse(gate.release_check_uri.as_deref().expect("uri"))
        .expect("release uri parse");
    assert!(app.fs.exists(&release_uri));
    let raw = app.fs.read(&release_uri).expect("read release check");
    let doc: ReleaseCheckDocument =
        serde_json::from_str(&raw).expect("parse release check document");
    assert_eq!(doc.gate_profile, "macmini-release");
    assert_eq!(doc.status, ReleaseGateStatus::Fail);
    assert_eq!(doc.embedding_provider.as_deref(), Some("semantic-lite"));
    assert!(doc.embedding_strict_error.is_none());
    assert!(doc.gate_record_uri.is_some());
}

#[test]
fn benchmark_gate_release_propagates_structured_embedding_strict_error() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_release_strict_input.txt");
    fs::write(&src, "OAuth release strict embedding propagation content.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-release-strict"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-release-strict"),
            Some(5),
            None,
            None,
        )
        .expect("find");

    let _ = app
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
            fixture_name: None,
        })
        .expect("bench");

    let mut reports = app.list_benchmark_reports(1).expect("list reports");
    let mut latest = reports.pop().expect("latest report");
    latest.environment.embedding_provider = Some("semantic-model-http".to_string());
    latest.environment.embedding_strict_error =
        Some("semantic-model-http embed request failed".to_string());
    let latest_uri = AxiomUri::parse(&latest.report_uri).expect("report uri");
    app.fs
        .write(
            &latest_uri,
            &serde_json::to_string_pretty(&latest).expect("serialize report"),
            true,
        )
        .expect("overwrite report");

    let gate = app
        .benchmark_gate_with_options(BenchmarkGateOptions {
            gate_profile: "macmini-release".to_string(),
            threshold_p95_ms: 10_000,
            min_top1_accuracy: 0.0,
            min_stress_top1_accuracy: None,
            max_p95_regression_pct: None,
            max_top1_regression_pct: None,
            window_size: 1,
            required_passes: 1,
            record: true,
            write_release_check: true,
        })
        .expect("profile gate");
    assert!(!gate.passed);
    assert_eq!(
        gate.embedding_provider.as_deref(),
        Some("semantic-model-http")
    );
    assert_eq!(
        gate.embedding_strict_error.as_deref(),
        Some("semantic-model-http embed request failed")
    );
    assert!(
        gate.reasons
            .iter()
            .any(|reason| { reason.starts_with("release_embedding_strict_error:") })
    );

    let release_uri = AxiomUri::parse(gate.release_check_uri.as_deref().expect("uri"))
        .expect("release uri parse");
    let raw = app.fs.read(&release_uri).expect("read release check");
    let doc: ReleaseCheckDocument =
        serde_json::from_str(&raw).expect("parse release check document");
    assert_eq!(
        doc.embedding_provider.as_deref(),
        Some("semantic-model-http")
    );
    assert_eq!(
        doc.embedding_strict_error.as_deref(),
        Some("semantic-model-http embed request failed")
    );
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
        .run_benchmark_suite(&BenchmarkRunOptions {
            query_limit: 10,
            search_limit: 5,
            include_golden: false,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
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
fn benchmark_amortized_mode_runs_multiple_iterations_in_process() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");
    app.initialize().expect("init failed");

    let src = temp.path().join("bench_amortized_input.txt");
    fs::write(&src, "OAuth benchmark amortized mode coverage.").expect("write input");
    app.add_resource(
        src.to_str().expect("src str"),
        Some("axiom://resources/bench-amortized"),
        None,
        None,
        true,
        None,
    )
    .expect("add failed");
    let _ = app
        .find(
            "oauth",
            Some("axiom://resources/bench-amortized"),
            Some(5),
            None,
            None,
        )
        .expect("find");

    let report = app
        .run_benchmark_suite_amortized(
            BenchmarkRunOptions {
                query_limit: 10,
                search_limit: 5,
                include_golden: false,
                include_trace: true,
                include_stress: true,
                trace_expectations: false,
                fixture_name: None,
            },
            3,
        )
        .expect("amortized benchmark");

    assert_eq!(report.mode, "in_process_amortized");
    assert_eq!(report.iterations, 3);
    assert_eq!(report.runs.len(), 3);
    assert!(report.executed_cases_total >= 3);
    assert!(report.wall_total_ms >= report.p95_latency_ms_median);
    assert!(report.p95_latency_us_median.is_some());
}
