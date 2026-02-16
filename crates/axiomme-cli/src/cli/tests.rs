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
            assert_eq!(mode, "strict");
        }
        _ => panic!("expected security audit command"),
    }
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
            assert_eq!(security_audit_mode, "strict");
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
