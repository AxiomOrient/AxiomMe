use std::fs;
use std::path::Path;

use tempfile::tempdir;

use super::command_needs_runtime;
use crate::cli::{
    AddArgs, BenchmarkArgs, BenchmarkCommand, Commands, DocumentArgs, DocumentCommand,
    DocumentMode, EvalArgs, EvalCommand, FindArgs, QueueArgs, QueueCommand, ReconcileArgs,
    TraceArgs, TraceCommand, WebArgs,
};
use axiomme_core::AxiomMe;

fn run(app: &AxiomMe, root: &Path, command: Commands) -> anyhow::Result<()> {
    super::validate_command_preflight(&command)?;
    super::run_validated(app, root, command)
}

#[test]
fn queue_status_does_not_require_runtime_prepare() {
    let command = Commands::Queue(QueueArgs {
        command: QueueCommand::Status,
    });
    assert!(!command_needs_runtime(&command));
}

#[test]
fn web_handoff_does_not_require_runtime_prepare() {
    let command = Commands::Web(WebArgs {
        host: "127.0.0.1".to_string(),
        port: 8787,
    });
    assert!(!command_needs_runtime(&command));
}

#[test]
fn find_requires_runtime_prepare() {
    let command = Commands::Find(crate::cli::FindArgs {
        query: "oauth".to_string(),
        target: None,
        limit: 10,
        budget_ms: None,
        budget_nodes: None,
        budget_depth: None,
    });
    assert!(command_needs_runtime(&command));
}

#[test]
fn backend_requires_runtime_prepare() {
    let command = Commands::Backend;
    assert!(command_needs_runtime(&command));
}

#[test]
fn backend_runs_runtime_prepare_and_reflects_local_records() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    run(&app, temp.path(), Commands::Init).expect("init");

    let source_path = temp.path().join("backend.md");
    fs::write(&source_path, "# Backend\n\nruntime index probe").expect("write source");
    run(
        &app,
        temp.path(),
        Commands::Add(AddArgs {
            source: source_path.to_string_lossy().to_string(),
            target: Some("axiom://resources/backend".to_string()),
            wait: false,
            markdown_only: false,
            include_hidden: false,
            exclude: Vec::new(),
        }),
    )
    .expect("add");

    let before = app.backend_status().expect("backend before");
    assert_eq!(before.local_records, 0);

    run(&app, temp.path(), Commands::Backend).expect("backend command");
    let after = app.backend_status().expect("backend after");
    assert!(after.local_records > 0);
}

#[test]
fn search_runs_runtime_prepare_for_memory_backend() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");

    let command = Commands::Search(crate::cli::SearchArgs {
        query: "oauth".to_string(),
        target: Some("axiom://resources".to_string()),
        session: None,
        limit: 5,
        score_threshold: None,
        min_match_tokens: None,
        budget_ms: None,
        budget_nodes: None,
        budget_depth: None,
    });
    run(&app, temp.path(), command).expect("search");

    assert!(
        temp.path().join("resources").join(".abstract.md").exists(),
        "memory search should run runtime prepare and synthesize root tiers"
    );
}

#[test]
fn trace_replay_requires_runtime_prepare() {
    let command = Commands::Trace(TraceArgs {
        command: TraceCommand::Replay {
            trace_id: "t-1".to_string(),
            limit: Some(5),
        },
    });
    assert!(command_needs_runtime(&command));
}

#[test]
fn benchmark_gate_does_not_require_runtime_prepare() {
    let command = Commands::Benchmark(BenchmarkArgs {
        command: BenchmarkCommand::Gate {
            threshold_p95_ms: 600,
            min_top1_accuracy: 0.75,
            min_stress_top1_accuracy: None,
            gate_profile: "custom".to_string(),
            max_p95_regression_pct: None,
            max_top1_regression_pct: None,
            window_size: 1,
            required_passes: 1,
            record: true,
            write_release_check: false,
            enforce: false,
        },
    });
    assert!(!command_needs_runtime(&command));
}

#[test]
fn add_ingest_options_require_markdown_only_for_exclude() {
    let err = super::build_add_ingest_options(false, false, &["**/*.json".to_string()])
        .expect_err("exclude without markdown-only must fail");
    assert!(format!("{err:#}").contains("--exclude requires --markdown-only"));
}

#[test]
fn add_ingest_options_markdown_only_defaults_are_applied() {
    let options =
        super::build_add_ingest_options(true, false, &["*.bak".to_string(), "  ".to_string()])
            .expect("options");
    assert!(options.markdown_only);
    assert!(!options.include_hidden);
    assert!(options.exclude_globs.iter().any(|x| x == "**/*.json"));
    assert!(options.exclude_globs.iter().any(|x| x == ".obsidian/**"));
    assert!(options.exclude_globs.iter().any(|x| x == "*.bak"));
    assert!(!options.exclude_globs.iter().any(|x| x.is_empty()));
}

#[test]
fn eval_run_requires_runtime_prepare() {
    let command = Commands::Eval(EvalArgs {
        command: EvalCommand::Run {
            trace_limit: 100,
            query_limit: 50,
            search_limit: 10,
            include_golden: true,
            golden_only: false,
        },
    });
    assert!(command_needs_runtime(&command));
}

#[test]
fn queue_status_uses_bootstrap_only_without_generating_root_tiers() {
    // Given a fresh root.
    // When running a queue status command.
    // Then CLI should only bootstrap and avoid runtime tier synthesis side effects.
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");

    let command = Commands::Queue(QueueArgs {
        command: QueueCommand::Status,
    });
    run(&app, temp.path(), command).expect("queue status");

    assert!(temp.path().join("resources").exists());
    assert!(!temp.path().join("resources").join(".abstract.md").exists());
}

#[test]
fn init_bootstraps_required_scope_directories() {
    // Given a fresh root.
    // When running `init`.
    // Then bootstrap should materialize required scope directories.
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    run(&app, temp.path(), Commands::Init).expect("init");

    assert!(temp.path().join("resources").exists());
    assert!(temp.path().join("queue").exists());
    assert!(temp.path().join("temp").exists());
}

#[test]
fn find_runs_runtime_prepare_and_generates_root_tiers() {
    // Given a fresh root.
    // When running retrieval (`find`).
    // Then runtime preparation must happen and root tiers should exist.
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");

    let command = Commands::Find(FindArgs {
        query: "oauth".to_string(),
        target: Some("axiom://resources".to_string()),
        limit: 5,
        budget_ms: None,
        budget_nodes: None,
        budget_depth: None,
    });
    run(&app, temp.path(), command).expect("find");

    assert!(temp.path().join("resources").join(".abstract.md").exists());
}

#[test]
fn document_save_requires_exactly_one_content_source() {
    // Given `document save` command.
    // When no source or multiple sources are provided.
    // Then CLI must fail before core write logic.
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    run(&app, temp.path(), Commands::Init).expect("init");

    let no_source = run(
        &app,
        temp.path(),
        Commands::Document(DocumentArgs {
            command: DocumentCommand::Save {
                uri: "axiom://resources/docs/guide.md".to_string(),
                mode: DocumentMode::Document,
                content: None,
                from: None,
                stdin: false,
                expected_etag: None,
            },
        }),
    )
    .expect_err("must fail without source");
    assert!(format!("{no_source:#}").contains("content source is required"));

    let from_path = temp.path().join("guide.md");
    fs::write(&from_path, "# guide").expect("write source file");
    let many_sources = run(
        &app,
        temp.path(),
        Commands::Document(DocumentArgs {
            command: DocumentCommand::Save {
                uri: "axiom://resources/docs/guide.md".to_string(),
                mode: DocumentMode::Document,
                content: Some("inline".to_string()),
                from: Some(from_path),
                stdin: false,
                expected_etag: None,
            },
        }),
    )
    .expect_err("must fail with multiple sources");
    assert!(format!("{many_sources:#}").contains("accepts exactly one content source"));
}

#[test]
fn document_preview_requires_exactly_one_source() {
    // Given `document preview` command.
    // When source selection is ambiguous or absent.
    // Then CLI must stop with explicit validation error.
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    run(&app, temp.path(), Commands::Init).expect("init");

    let no_source = run(
        &app,
        temp.path(),
        Commands::Document(DocumentArgs {
            command: DocumentCommand::Preview {
                uri: None,
                content: None,
                from: None,
                stdin: false,
            },
        }),
    )
    .expect_err("must fail without preview source");
    assert!(format!("{no_source:#}").contains("preview source is required"));

    let from_path = temp.path().join("guide.md");
    fs::write(&from_path, "# guide").expect("write source file");
    let many_sources = run(
        &app,
        temp.path(),
        Commands::Document(DocumentArgs {
            command: DocumentCommand::Preview {
                uri: Some("axiom://resources/docs/guide.md".to_string()),
                content: None,
                from: Some(from_path),
                stdin: false,
            },
        }),
    )
    .expect_err("must fail with multiple preview sources");
    assert!(format!("{many_sources:#}").contains("accepts exactly one source"));
}

#[test]
fn benchmark_gate_enforce_propagates_failure_as_cli_error() {
    // Given no benchmark reports.
    // When running benchmark gate with enforce=true.
    // Then CLI must return an error (non-zero exit contract equivalent).
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");

    let err = run(
        &app,
        temp.path(),
        Commands::Benchmark(BenchmarkArgs {
            command: BenchmarkCommand::Gate {
                threshold_p95_ms: 600,
                min_top1_accuracy: 0.75,
                min_stress_top1_accuracy: None,
                gate_profile: "custom".to_string(),
                max_p95_regression_pct: None,
                max_top1_regression_pct: None,
                window_size: 1,
                required_passes: 1,
                record: false,
                write_release_check: false,
                enforce: true,
            },
        }),
    )
    .expect_err("must fail with enforce");
    assert!(format!("{err:#}").contains("benchmark gate failed"));
}

#[test]
fn document_preview_validation_runs_before_bootstrap_side_effects() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");

    let err = run(
        &app,
        temp.path(),
        Commands::Document(DocumentArgs {
            command: DocumentCommand::Preview {
                uri: None,
                content: None,
                from: None,
                stdin: false,
            },
        }),
    )
    .expect_err("must fail without source");
    assert!(format!("{err:#}").contains("preview source is required"));
    assert!(!temp.path().join("resources").exists());
}

#[test]
fn add_markdown_flag_validation_runs_before_bootstrap_side_effects() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");

    let err = run(
        &app,
        temp.path(),
        Commands::Add(AddArgs {
            source: "/tmp/does-not-matter".to_string(),
            target: Some("axiom://resources/invalid".to_string()),
            wait: false,
            markdown_only: false,
            include_hidden: false,
            exclude: vec!["**/*.json".to_string()],
        }),
    )
    .expect_err("must fail");
    assert!(format!("{err:#}").contains("--exclude requires --markdown-only"));
    assert!(!temp.path().join("resources").exists());
}

#[test]
fn reconcile_scope_validation_runs_before_bootstrap_side_effects() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");

    let err = run(
        &app,
        temp.path(),
        Commands::Reconcile(ReconcileArgs {
            dry_run: true,
            scopes: vec!["not-a-scope".to_string()],
            max_drift_sample: 50,
        }),
    )
    .expect_err("invalid scope must fail");
    assert!(format!("{err:#}").contains("invalid --scope value"));
    assert!(!temp.path().join("resources").exists());
}

#[test]
fn queue_work_zero_iterations_has_stable_mode_value() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    run(&app, temp.path(), Commands::Init).expect("init");

    let report = super::queue::run_queue_worker(&app, 0, 10, 0, false, true).expect("report");
    let payload = serde_json::to_value(report).expect("serialize");
    assert_eq!(payload["mode"], "work");
    assert_eq!(payload["iterations"], 0);
}

#[test]
fn queue_daemon_zero_max_cycles_still_reports_daemon_mode() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    run(&app, temp.path(), Commands::Init).expect("init");

    let report = super::queue::run_queue_daemon(&app, 0, 10, 0, false, true, 1).expect("report");
    let payload = serde_json::to_value(report).expect("serialize");
    assert_eq!(payload["mode"], "daemon");
}

#[test]
fn benchmark_gate_rejects_required_passes_over_window_before_runtime_prepare() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");

    let err = run(
        &app,
        temp.path(),
        Commands::Benchmark(BenchmarkArgs {
            command: BenchmarkCommand::Gate {
                threshold_p95_ms: 600,
                min_top1_accuracy: 0.75,
                min_stress_top1_accuracy: None,
                gate_profile: "custom".to_string(),
                max_p95_regression_pct: None,
                max_top1_regression_pct: None,
                window_size: 1,
                required_passes: 2,
                record: false,
                write_release_check: false,
                enforce: false,
            },
        }),
    )
    .expect_err("invalid gate policy must fail");

    assert!(format!("{err:#}").contains("--required-passes (2) cannot exceed --window-size (1)"));
    assert!(!temp.path().join("resources").exists());
}
