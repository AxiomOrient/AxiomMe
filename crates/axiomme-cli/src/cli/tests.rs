use super::*;
use clap::Parser;

#[test]
fn queue_status_parses_as_read_only_status_command() {
    let cli = Cli::try_parse_from(["axiomme", "queue", "status"]).expect("parse");
    match cli.command {
        Commands::Queue(QueueArgs {
            command: QueueCommand::Status,
        }) => {}
        _ => panic!("expected queue status command"),
    }
}

#[test]
fn queue_wait_parses_timeout_option() {
    let cli =
        Cli::try_parse_from(["axiomme", "queue", "wait", "--timeout-secs", "7"]).expect("parse");
    match cli.command {
        Commands::Queue(QueueArgs {
            command: QueueCommand::Wait { timeout_secs },
        }) => {
            assert_eq!(timeout_secs, Some(7));
        }
        _ => panic!("expected queue wait command"),
    }
}

#[test]
fn queue_inspect_is_no_longer_supported() {
    let parsed = Cli::try_parse_from(["axiomme", "queue", "inspect"]);
    assert!(parsed.is_err(), "queue inspect must be rejected");
}

#[test]
fn ontology_validate_parses_optional_uri() {
    let cli = Cli::try_parse_from([
        "axiomme",
        "ontology",
        "validate",
        "--uri",
        "axiom://agent/ontology/schema.v1.json",
    ])
    .expect("parse");
    match cli.command {
        Commands::Ontology(OntologyArgs {
            command: OntologyCommand::Validate { uri },
        }) => {
            assert_eq!(
                uri.as_deref(),
                Some("axiom://agent/ontology/schema.v1.json")
            );
        }
        _ => panic!("expected ontology validate command"),
    }
}

#[test]
fn ontology_pressure_parses_thresholds() {
    let cli = Cli::try_parse_from([
        "axiomme",
        "ontology",
        "pressure",
        "--uri",
        "axiom://agent/ontology/schema.v1.json",
        "--min-action-types",
        "4",
        "--min-invariants",
        "5",
        "--min-action-invariant-total",
        "9",
        "--min-link-types-per-object-basis-points",
        "12000",
    ])
    .expect("parse");
    match cli.command {
        Commands::Ontology(OntologyArgs {
            command:
                OntologyCommand::Pressure {
                    uri,
                    min_action_types,
                    min_invariants,
                    min_action_invariant_total,
                    min_link_types_per_object_basis_points,
                },
        }) => {
            assert_eq!(
                uri.as_deref(),
                Some("axiom://agent/ontology/schema.v1.json")
            );
            assert_eq!(min_action_types, 4);
            assert_eq!(min_invariants, 5);
            assert_eq!(min_action_invariant_total, 9);
            assert_eq!(min_link_types_per_object_basis_points, 12000);
        }
        _ => panic!("expected ontology pressure command"),
    }
}

#[test]
fn ontology_trend_parses_history_and_thresholds() {
    let cli = Cli::try_parse_from([
        "axiomme",
        "ontology",
        "trend",
        "--history-dir",
        "/tmp/ontology-pressure",
        "--min-samples",
        "4",
        "--consecutive-v2-candidate",
        "3",
    ])
    .expect("parse");
    match cli.command {
        Commands::Ontology(OntologyArgs {
            command:
                OntologyCommand::Trend {
                    history_dir,
                    min_samples,
                    consecutive_v2_candidate,
                },
        }) => {
            assert_eq!(history_dir.to_string_lossy(), "/tmp/ontology-pressure");
            assert_eq!(min_samples, 4);
            assert_eq!(consecutive_v2_candidate, 3);
        }
        _ => panic!("expected ontology trend command"),
    }
}

#[test]
fn ontology_trend_rejects_zero_thresholds() {
    let min_samples_error = Cli::try_parse_from([
        "axiomme",
        "ontology",
        "trend",
        "--history-dir",
        "/tmp/ontology-pressure",
        "--min-samples",
        "0",
    ]);
    assert!(min_samples_error.is_err(), "min-samples=0 must be rejected");

    let consecutive_error = Cli::try_parse_from([
        "axiomme",
        "ontology",
        "trend",
        "--history-dir",
        "/tmp/ontology-pressure",
        "--consecutive-v2-candidate",
        "0",
    ]);
    assert!(
        consecutive_error.is_err(),
        "consecutive-v2-candidate=0 must be rejected"
    );
}

#[test]
fn ontology_action_validate_parses_input_sources() {
    let cli = Cli::try_parse_from([
        "axiomme",
        "ontology",
        "action-validate",
        "--action-id",
        "sync_doc",
        "--queue-event-type",
        "semantic_scan",
        "--input-json",
        "{\"uri\":\"axiom://resources/docs/a.md\"}",
    ])
    .expect("parse");
    match cli.command {
        Commands::Ontology(OntologyArgs {
            command:
                OntologyCommand::ActionValidate {
                    action_id,
                    queue_event_type,
                    input_json,
                    input_file,
                    input_stdin,
                    ..
                },
        }) => {
            assert_eq!(action_id, "sync_doc");
            assert_eq!(queue_event_type, "semantic_scan");
            assert_eq!(
                input_json.as_deref(),
                Some("{\"uri\":\"axiom://resources/docs/a.md\"}")
            );
            assert!(input_file.is_none());
            assert!(!input_stdin);
        }
        _ => panic!("expected ontology action-validate command"),
    }
}

#[test]
fn ontology_action_enqueue_parses_target_uri() {
    let cli = Cli::try_parse_from([
        "axiomme",
        "ontology",
        "action-enqueue",
        "--target-uri",
        "axiom://resources/docs/a.md",
        "--action-id",
        "sync_doc",
        "--queue-event-type",
        "semantic_scan",
    ])
    .expect("parse");
    match cli.command {
        Commands::Ontology(OntologyArgs {
            command:
                OntologyCommand::ActionEnqueue {
                    target_uri,
                    action_id,
                    queue_event_type,
                    input_json,
                    input_file,
                    input_stdin,
                    ..
                },
        }) => {
            assert_eq!(target_uri, "axiom://resources/docs/a.md");
            assert_eq!(action_id, "sync_doc");
            assert_eq!(queue_event_type, "semantic_scan");
            assert!(input_json.is_none());
            assert!(input_file.is_none());
            assert!(!input_stdin);
        }
        _ => panic!("expected ontology action-enqueue command"),
    }
}

#[test]
fn ontology_invariant_check_parses_enforce_flag() {
    let cli = Cli::try_parse_from([
        "axiomme",
        "ontology",
        "invariant-check",
        "--uri",
        "axiom://agent/ontology/schema.v1.json",
        "--enforce",
    ])
    .expect("parse");
    match cli.command {
        Commands::Ontology(OntologyArgs {
            command: OntologyCommand::InvariantCheck { uri, enforce },
        }) => {
            assert_eq!(
                uri.as_deref(),
                Some("axiom://agent/ontology/schema.v1.json")
            );
            assert!(enforce);
        }
        _ => panic!("expected ontology invariant-check command"),
    }
}

#[test]
fn document_save_from_file_parses() {
    let cli = Cli::try_parse_from([
        "axiomme",
        "document",
        "save",
        "axiom://resources/docs/guide.md",
        "--from",
        "guide.md",
    ])
    .expect("parse");

    match cli.command {
        Commands::Document(DocumentArgs {
            command:
                DocumentCommand::Save {
                    uri,
                    from,
                    content,
                    stdin,
                    ..
                },
        }) => {
            assert_eq!(uri, "axiom://resources/docs/guide.md");
            assert_eq!(from.as_deref().and_then(|x| x.to_str()), Some("guide.md"));
            assert!(content.is_none());
            assert!(!stdin);
        }
        _ => panic!("expected document save"),
    }
}

#[test]
fn document_preview_from_uri_parses() {
    let cli = Cli::try_parse_from([
        "axiomme",
        "document",
        "preview",
        "--uri",
        "axiom://resources/docs/guide.md",
    ])
    .expect("parse");

    match cli.command {
        Commands::Document(DocumentArgs {
            command:
                DocumentCommand::Preview {
                    uri,
                    content,
                    from,
                    stdin,
                },
        }) => {
            assert_eq!(uri.as_deref(), Some("axiom://resources/docs/guide.md"));
            assert!(content.is_none());
            assert!(from.is_none());
            assert!(!stdin);
        }
        _ => panic!("expected document preview"),
    }
}

#[test]
fn benchmark_amortized_parses_iterations() {
    let cli = Cli::try_parse_from([
        "axiomme",
        "benchmark",
        "amortized",
        "--iterations",
        "5",
        "--query-limit",
        "25",
    ])
    .expect("parse");

    match cli.command {
        Commands::Benchmark(BenchmarkArgs {
            command:
                BenchmarkCommand::Amortized {
                    iterations,
                    query_limit,
                    ..
                },
        }) => {
            assert_eq!(iterations, 5);
            assert_eq!(query_limit, 25);
        }
        _ => panic!("expected benchmark amortized command"),
    }
}

#[test]
fn benchmark_gate_parses_min_stress_top1_accuracy() {
    let cli = Cli::try_parse_from([
        "axiomme",
        "benchmark",
        "gate",
        "--min-stress-top1-accuracy",
        "0.65",
    ])
    .expect("parse");

    match cli.command {
        Commands::Benchmark(BenchmarkArgs {
            command:
                BenchmarkCommand::Gate {
                    min_stress_top1_accuracy,
                    ..
                },
        }) => {
            assert_eq!(min_stress_top1_accuracy, Some(0.65));
        }
        _ => panic!("expected benchmark gate command"),
    }
}

#[test]
fn release_pack_parses_benchmark_min_stress_top1_accuracy() {
    let cli = Cli::try_parse_from([
        "axiomme",
        "release",
        "pack",
        "--benchmark-min-stress-top1-accuracy",
        "0.7",
    ])
    .expect("parse");

    match cli.command {
        Commands::Release(ReleaseArgs {
            command:
                ReleaseCommand::Pack {
                    benchmark_min_stress_top1_accuracy,
                    ..
                },
        }) => {
            assert_eq!(benchmark_min_stress_top1_accuracy, Some(0.7));
        }
        _ => panic!("expected release pack command"),
    }
}

#[test]
fn security_audit_parses_mode() {
    let cli =
        Cli::try_parse_from(["axiomme", "security", "audit", "--mode", "strict"]).expect("parse");

    match cli.command {
        Commands::Security(SecurityArgs {
            command: SecurityCommand::Audit { mode, .. },
        }) => {
            assert!(matches!(mode, SecurityAuditModeArg::Strict));
        }
        _ => panic!("expected security audit command"),
    }
}

#[test]
fn security_audit_rejects_unknown_mode() {
    let parsed = Cli::try_parse_from(["axiomme", "security", "audit", "--mode", "invalid"]);
    assert!(parsed.is_err(), "unknown security audit mode must fail");
}

#[test]
fn release_pack_defaults_security_audit_mode_to_strict() {
    let cli = Cli::try_parse_from(["axiomme", "release", "pack"]).expect("parse");

    match cli.command {
        Commands::Release(ReleaseArgs {
            command:
                ReleaseCommand::Pack {
                    security_audit_mode,
                    ..
                },
        }) => {
            assert!(matches!(
                security_audit_mode,
                ReleaseSecurityAuditModeArg::Strict
            ));
        }
        _ => panic!("expected release pack command"),
    }
}

#[test]
fn add_parses_markdown_only_filter_flags() {
    let cli = Cli::try_parse_from([
        "axiomme",
        "add",
        "/tmp/vault",
        "--markdown-only",
        "--exclude",
        "**/*.json",
    ])
    .expect("parse");

    match cli.command {
        Commands::Add(AddArgs {
            source,
            markdown_only,
            include_hidden,
            exclude,
            ..
        }) => {
            assert_eq!(source, "/tmp/vault");
            assert!(markdown_only);
            assert!(!include_hidden);
            assert_eq!(exclude, vec!["**/*.json".to_string()]);
        }
        _ => panic!("expected add command"),
    }
}

#[test]
fn search_parses_score_and_min_match_options() {
    let cli = Cli::try_parse_from([
        "axiomme",
        "search",
        "oauth",
        "--score-threshold",
        "0.35",
        "--min-match-tokens",
        "2",
    ])
    .expect("parse");

    match cli.command {
        Commands::Search(SearchArgs {
            query,
            score_threshold,
            min_match_tokens,
            ..
        }) => {
            assert_eq!(query, "oauth");
            assert_eq!(score_threshold, Some(0.35));
            assert_eq!(min_match_tokens, Some(2));
        }
        _ => panic!("expected search command"),
    }
}

#[test]
fn search_rejects_out_of_range_score_threshold() {
    let parsed = Cli::try_parse_from(["axiomme", "search", "oauth", "--score-threshold", "1.5"]);
    assert!(
        parsed.is_err(),
        "score threshold above 1.0 must be rejected"
    );
}

#[test]
fn search_rejects_min_match_tokens_below_two() {
    let parsed = Cli::try_parse_from([
        "axiomme",
        "search",
        "oauth callback",
        "--min-match-tokens",
        "1",
    ]);
    assert!(parsed.is_err(), "min-match-tokens below 2 must be rejected");
}

#[test]
fn queue_daemon_rejects_zero_idle_cycles() {
    let parsed = Cli::try_parse_from(["axiomme", "queue", "daemon", "--idle-cycles", "0"]);
    assert!(parsed.is_err(), "idle-cycles must be >= 1");
}

#[test]
fn benchmark_gate_rejects_nan_min_top1_accuracy() {
    let parsed =
        Cli::try_parse_from(["axiomme", "benchmark", "gate", "--min-top1-accuracy", "NaN"]);
    assert!(parsed.is_err(), "NaN threshold must be rejected");
}

#[test]
fn benchmark_gate_rejects_zero_window_size() {
    let parsed = Cli::try_parse_from(["axiomme", "benchmark", "gate", "--window-size", "0"]);
    assert!(parsed.is_err(), "window-size must be >= 1");
}

#[test]
fn benchmark_gate_rejects_zero_required_passes() {
    let parsed = Cli::try_parse_from(["axiomme", "benchmark", "gate", "--required-passes", "0"]);
    assert!(parsed.is_err(), "required-passes must be >= 1");
}

#[test]
fn benchmark_gate_rejects_negative_regression_threshold() {
    let parsed = Cli::try_parse_from([
        "axiomme",
        "benchmark",
        "gate",
        "--max-p95-regression-pct",
        "-1",
    ]);
    assert!(
        parsed.is_err(),
        "negative regression threshold must be rejected"
    );
}

#[test]
fn release_pack_rejects_nan_benchmark_min_top1_accuracy() {
    let parsed = Cli::try_parse_from([
        "axiomme",
        "release",
        "pack",
        "--benchmark-min-top1-accuracy",
        "NaN",
    ]);
    assert!(parsed.is_err(), "NaN benchmark threshold must be rejected");
}

#[test]
fn release_pack_rejects_zero_benchmark_window_size() {
    let parsed =
        Cli::try_parse_from(["axiomme", "release", "pack", "--benchmark-window-size", "0"]);
    assert!(parsed.is_err(), "benchmark-window-size must be >= 1");
}
