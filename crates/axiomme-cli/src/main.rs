use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use axiomme_core::models::{
    BenchmarkGateOptions, MetadataFilter, ReleaseGatePackOptions, SearchBudget,
};
use axiomme_core::{AxiomMe, BenchmarkRunOptions, EvalRunOptions, ReconcileOptions, Scope};
use clap::{Args, Parser, Subcommand};

mod web;

#[derive(Debug, Parser)]
#[command(name = "axiomme")]
#[command(about = "Personal Axiom-compatible context database", version)]
struct Cli {
    #[arg(long, default_value = ".axiomme")]
    root: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init,
    Add(AddArgs),
    Wait,
    Ls(ListArgs),
    Glob(GlobArgs),
    Read(UriArg),
    Abstract(UriArg),
    Overview(UriArg),
    Find(FindArgs),
    Search(SearchArgs),
    Backend,
    Queue(QueueArgs),
    Trace(TraceArgs),
    Eval(EvalArgs),
    Benchmark(BenchmarkArgs),
    Security(SecurityArgs),
    Release(ReleaseArgs),
    Reconcile(ReconcileArgs),
    Session(SessionArgs),
    ExportOvpack(ExportArgs),
    ImportOvpack(ImportArgs),
    Web(WebArgs),
}

#[derive(Debug, Args)]
struct AddArgs {
    source: String,
    #[arg(long)]
    target: Option<String>,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    wait: bool,
}

#[derive(Debug, Args)]
struct ListArgs {
    uri: String,
    #[arg(short, long)]
    recursive: bool,
}

#[derive(Debug, Args)]
struct GlobArgs {
    pattern: String,
    #[arg(long)]
    uri: Option<String>,
}

#[derive(Debug, Args)]
struct UriArg {
    uri: String,
}

#[derive(Debug, Args)]
struct FindArgs {
    query: String,
    #[arg(long)]
    target: Option<String>,
    #[arg(long, default_value_t = 10)]
    limit: usize,
    #[arg(long)]
    budget_ms: Option<u64>,
    #[arg(long)]
    budget_nodes: Option<usize>,
    #[arg(long)]
    budget_depth: Option<usize>,
}

#[derive(Debug, Args)]
struct SearchArgs {
    query: String,
    #[arg(long)]
    target: Option<String>,
    #[arg(long)]
    session: Option<String>,
    #[arg(long, default_value_t = 10)]
    limit: usize,
    #[arg(long)]
    budget_ms: Option<u64>,
    #[arg(long)]
    budget_nodes: Option<usize>,
    #[arg(long)]
    budget_depth: Option<usize>,
}

#[derive(Debug, Args)]
struct SessionArgs {
    #[command(subcommand)]
    command: SessionCommand,
}

#[derive(Debug, Args)]
struct QueueArgs {
    #[command(subcommand)]
    command: QueueCommand,
}

#[derive(Debug, Args)]
struct TraceArgs {
    #[command(subcommand)]
    command: TraceCommand,
}

#[derive(Debug, Args)]
struct ReconcileArgs {
    #[arg(long, default_value_t = false)]
    dry_run: bool,
    #[arg(long = "scope")]
    scopes: Vec<String>,
    #[arg(long, default_value_t = 50)]
    max_drift_sample: usize,
}

#[derive(Debug, Args)]
struct EvalArgs {
    #[command(subcommand)]
    command: EvalCommand,
}

#[derive(Debug, Args)]
struct BenchmarkArgs {
    #[command(subcommand)]
    command: BenchmarkCommand,
}

#[derive(Debug, Args)]
struct SecurityArgs {
    #[command(subcommand)]
    command: SecurityCommand,
}

#[derive(Debug, Args)]
struct ReleaseArgs {
    #[command(subcommand)]
    command: ReleaseCommand,
}

#[derive(Debug, Subcommand)]
enum QueueCommand {
    Status,
    Inspect,
    Replay {
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value_t = false)]
        include_dead_letter: bool,
    },
    Work {
        #[arg(long, default_value_t = 20)]
        iterations: u32,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value_t = 500)]
        sleep_ms: u64,
        #[arg(long, default_value_t = false)]
        include_dead_letter: bool,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        stop_when_idle: bool,
    },
    Daemon {
        #[arg(long, default_value_t = 120)]
        max_cycles: u32,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value_t = 1000)]
        sleep_ms: u64,
        #[arg(long, default_value_t = false)]
        include_dead_letter: bool,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        stop_when_idle: bool,
        #[arg(long, default_value_t = 3)]
        idle_cycles: u32,
    },
    Evidence {
        #[arg(long, default_value_t = 100)]
        replay_limit: usize,
        #[arg(long, default_value_t = 8)]
        max_cycles: u32,
        #[arg(long, default_value_t = false)]
        enforce: bool,
    },
}

#[derive(Debug, Subcommand)]
enum TraceCommand {
    Requests {
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long)]
        operation: Option<String>,
        #[arg(long)]
        status: Option<String>,
    },
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Get {
        trace_id: String,
    },
    Replay {
        trace_id: String,
        #[arg(long)]
        limit: Option<usize>,
    },
    Stats {
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value_t = false)]
        include_replays: bool,
    },
    Snapshot {
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value_t = false)]
        include_replays: bool,
    },
    Snapshots {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Trend {
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        request_type: Option<String>,
    },
    Evidence {
        #[arg(long, default_value_t = 100)]
        trace_limit: usize,
        #[arg(long, default_value_t = 100)]
        request_limit: usize,
        #[arg(long, default_value_t = false)]
        enforce: bool,
    },
}

#[derive(Debug, Subcommand)]
enum EvalCommand {
    Run {
        #[arg(long, default_value_t = 100)]
        trace_limit: usize,
        #[arg(long, default_value_t = 50)]
        query_limit: usize,
        #[arg(long, default_value_t = 10)]
        search_limit: usize,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_golden: bool,
        #[arg(long, default_value_t = false)]
        golden_only: bool,
    },
    Golden {
        #[command(subcommand)]
        command: EvalGoldenCommand,
    },
}

#[derive(Debug, Subcommand)]
enum EvalGoldenCommand {
    List,
    Add {
        #[arg(long)]
        query: String,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        expected_top: Option<String>,
    },
    MergeFromTraces {
        #[arg(long, default_value_t = 200)]
        trace_limit: usize,
        #[arg(long, default_value_t = 100)]
        max_add: usize,
    },
}

#[derive(Debug, Subcommand)]
enum BenchmarkCommand {
    Run {
        #[arg(long, default_value_t = 100)]
        query_limit: usize,
        #[arg(long, default_value_t = 10)]
        search_limit: usize,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_golden: bool,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_trace: bool,
        #[arg(long)]
        fixture_name: Option<String>,
    },
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Trend {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Gate {
        #[arg(long, default_value_t = 600)]
        threshold_p95_ms: u128,
        #[arg(long, default_value_t = 0.75)]
        min_top1_accuracy: f32,
        #[arg(long, default_value = "custom")]
        gate_profile: String,
        #[arg(long)]
        max_p95_regression_pct: Option<f32>,
        #[arg(long)]
        max_top1_regression_pct: Option<f32>,
        #[arg(long, default_value_t = 1)]
        window_size: usize,
        #[arg(long, default_value_t = 1)]
        required_passes: usize,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        record: bool,
        #[arg(long, default_value_t = false)]
        write_release_check: bool,
        #[arg(long, default_value_t = false)]
        enforce: bool,
    },
    Fixture {
        #[command(subcommand)]
        command: BenchmarkFixtureCommand,
    },
}

#[derive(Debug, Subcommand)]
enum BenchmarkFixtureCommand {
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = 100)]
        query_limit: usize,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_golden: bool,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_trace: bool,
    },
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

#[derive(Debug, Subcommand)]
enum SecurityCommand {
    Audit {
        #[arg(long)]
        workspace_dir: Option<String>,
        #[arg(long, default_value_t = false)]
        enforce: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ReleaseCommand {
    Pack {
        #[arg(long)]
        workspace_dir: Option<String>,
        #[arg(long, default_value_t = 100)]
        replay_limit: usize,
        #[arg(long, default_value_t = 8)]
        replay_max_cycles: u32,
        #[arg(long, default_value_t = 200)]
        trace_limit: usize,
        #[arg(long, default_value_t = 200)]
        request_limit: usize,
        #[arg(long, default_value_t = 200)]
        eval_trace_limit: usize,
        #[arg(long, default_value_t = 50)]
        eval_query_limit: usize,
        #[arg(long, default_value_t = 10)]
        eval_search_limit: usize,
        #[arg(long, default_value_t = 60)]
        benchmark_query_limit: usize,
        #[arg(long, default_value_t = 10)]
        benchmark_search_limit: usize,
        #[arg(long, default_value_t = 600)]
        benchmark_threshold_p95_ms: u128,
        #[arg(long, default_value_t = 0.75)]
        benchmark_min_top1_accuracy: f32,
        #[arg(long)]
        benchmark_max_p95_regression_pct: Option<f32>,
        #[arg(long)]
        benchmark_max_top1_regression_pct: Option<f32>,
        #[arg(long, default_value_t = 1)]
        benchmark_window_size: usize,
        #[arg(long, default_value_t = 1)]
        benchmark_required_passes: usize,
        #[arg(long, default_value_t = false)]
        enforce: bool,
    },
}

#[derive(Debug, Subcommand)]
enum SessionCommand {
    Create {
        #[arg(long)]
        id: Option<String>,
    },
    Add {
        #[arg(long)]
        id: String,
        #[arg(long)]
        role: String,
        #[arg(long)]
        text: String,
    },
    Commit {
        #[arg(long)]
        id: String,
    },
    List,
    Delete {
        #[arg(long)]
        id: String,
    },
}

#[derive(Debug, Args)]
struct ExportArgs {
    uri: String,
    to: String,
}

#[derive(Debug, Args)]
struct ImportArgs {
    file: String,
    parent: String,
    #[arg(long, default_value_t = false)]
    force: bool,
    #[arg(long, default_value_t = true)]
    vectorize: bool,
}

#[derive(Debug, Args)]
struct WebArgs {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 8787)]
    port: u16,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let app = AxiomMe::new(&cli.root).context("failed to create app")?;

    match cli.command {
        Commands::Init => {
            app.initialize().context("init failed")?;
            println!("initialized at {}", cli.root.display());
        }
        Commands::Add(args) => {
            app.initialize().context("init failed")?;
            let result = app.add_resource(
                &args.source,
                args.target.as_deref(),
                None,
                None,
                args.wait,
                None,
            )?;
            print_json(&result)?;
        }
        Commands::Wait => {
            app.initialize().context("init failed")?;
            let status = app.wait_processed(None)?;
            print_json(&status)?;
        }
        Commands::Ls(args) => {
            app.initialize().context("init failed")?;
            let entries = app.ls(&args.uri, args.recursive, false)?;
            print_json(&entries)?;
        }
        Commands::Glob(args) => {
            app.initialize().context("init failed")?;
            let result = app.glob(&args.pattern, args.uri.as_deref())?;
            print_json(&result)?;
        }
        Commands::Read(args) => {
            app.initialize().context("init failed")?;
            println!("{}", app.read(&args.uri)?);
        }
        Commands::Abstract(args) => {
            app.initialize().context("init failed")?;
            println!("{}", app.abstract_text(&args.uri)?);
        }
        Commands::Overview(args) => {
            app.initialize().context("init failed")?;
            println!("{}", app.overview(&args.uri)?);
        }
        Commands::Find(args) => {
            app.initialize().context("init failed")?;
            let budget = parse_search_budget(args.budget_ms, args.budget_nodes, args.budget_depth);
            let result = app.find_with_budget(
                &args.query,
                args.target.as_deref(),
                Some(args.limit),
                None,
                None::<MetadataFilter>,
                budget,
            )?;
            print_json(&result)?;
        }
        Commands::Search(args) => {
            app.initialize().context("init failed")?;
            let budget = parse_search_budget(args.budget_ms, args.budget_nodes, args.budget_depth);
            let result = app.search_with_budget(
                &args.query,
                args.target.as_deref(),
                args.session.as_deref(),
                Some(args.limit),
                None,
                None::<MetadataFilter>,
                budget,
            )?;
            print_json(&result)?;
        }
        Commands::Backend => {
            app.initialize().context("init failed")?;
            let status = app.backend_status()?;
            print_json(&status)?;
        }
        Commands::Queue(args) => match args.command {
            QueueCommand::Status => {
                let status = app.wait_processed(None)?;
                print_json(&status)?;
            }
            QueueCommand::Inspect => {
                let diagnostics = app.queue_diagnostics()?;
                print_json(&diagnostics)?;
            }
            QueueCommand::Replay {
                limit,
                include_dead_letter,
            } => {
                let report = app.replay_outbox(limit, include_dead_letter)?;
                print_json(&report)?;
            }
            QueueCommand::Work {
                iterations,
                limit,
                sleep_ms,
                include_dead_letter,
                stop_when_idle,
            } => {
                let report = run_queue_worker(
                    &app,
                    iterations,
                    limit,
                    sleep_ms,
                    include_dead_letter,
                    stop_when_idle,
                )?;
                print_json(&report)?;
            }
            QueueCommand::Daemon {
                max_cycles,
                limit,
                sleep_ms,
                include_dead_letter,
                stop_when_idle,
                idle_cycles,
            } => {
                let report = run_queue_daemon(
                    &app,
                    max_cycles,
                    limit,
                    sleep_ms,
                    include_dead_letter,
                    stop_when_idle,
                    idle_cycles,
                )?;
                print_json(&report)?;
            }
            QueueCommand::Evidence {
                replay_limit,
                max_cycles,
                enforce,
            } => {
                let report = app.collect_reliability_evidence(replay_limit, max_cycles)?;
                print_json(&report)?;
                if enforce && !report.passed {
                    anyhow::bail!("reliability evidence checks failed");
                }
            }
        },
        Commands::Trace(args) => {
            app.initialize().context("init failed")?;
            handle_trace(&app, args.command)?;
        }
        Commands::Eval(args) => {
            app.initialize().context("init failed")?;
            handle_eval(&app, args.command)?;
        }
        Commands::Benchmark(args) => {
            app.initialize().context("init failed")?;
            handle_benchmark(&app, args.command)?;
        }
        Commands::Security(args) => {
            app.initialize().context("init failed")?;
            handle_security(&app, args.command)?;
        }
        Commands::Release(args) => {
            app.initialize().context("init failed")?;
            handle_release(&app, args.command)?;
        }
        Commands::Reconcile(args) => {
            app.initialize().context("init failed")?;
            let scopes = parse_scope_args(&args.scopes)?;
            let report = app.reconcile_state_with_options(ReconcileOptions {
                dry_run: args.dry_run,
                scopes,
                max_drift_sample: args.max_drift_sample,
            })?;
            print_json(&report)?;
        }
        Commands::Session(args) => {
            app.initialize().context("init failed")?;
            handle_session(&app, args.command)?;
        }
        Commands::ExportOvpack(args) => {
            app.initialize().context("init failed")?;
            let out = app.export_ovpack(&args.uri, &args.to)?;
            println!("{}", out);
        }
        Commands::ImportOvpack(args) => {
            app.initialize().context("init failed")?;
            let out = app.import_ovpack(&args.file, &args.parent, args.force, args.vectorize)?;
            println!("{}", out);
        }
        Commands::Web(args) => {
            app.initialize().context("init failed")?;
            web::serve_web(app.clone(), &args.host, args.port)?;
        }
    }

    Ok(())
}

fn handle_session(app: &AxiomMe, command: SessionCommand) -> Result<()> {
    match command {
        SessionCommand::Create { id } => {
            let session = app.session(id.as_deref());
            session.load()?;
            println!("{}", session.session_id);
        }
        SessionCommand::Add { id, role, text } => {
            let session = app.session(Some(&id));
            session.load()?;
            let message = session.add_message(&role, text)?;
            print_json(&message)?;
        }
        SessionCommand::Commit { id } => {
            let session = app.session(Some(&id));
            session.load()?;
            let result = session.commit()?;
            print_json(&result)?;
        }
        SessionCommand::List => {
            let sessions = app.sessions()?;
            print_json(&sessions)?;
        }
        SessionCommand::Delete { id } => {
            let deleted = app.delete(&id)?;
            println!("{}", deleted);
        }
    }
    Ok(())
}

fn handle_trace(app: &AxiomMe, command: TraceCommand) -> Result<()> {
    match command {
        TraceCommand::Requests {
            limit,
            operation,
            status,
        } => {
            let logs =
                app.list_request_logs_filtered(limit, operation.as_deref(), status.as_deref())?;
            print_json(&logs)?;
        }
        TraceCommand::List { limit } => {
            let traces = app.list_traces(limit)?;
            print_json(&traces)?;
        }
        TraceCommand::Get { trace_id } => {
            let trace = app.get_trace(&trace_id)?;
            print_json(&trace)?;
        }
        TraceCommand::Replay { trace_id, limit } => {
            let replay = app.replay_trace(&trace_id, limit)?;
            print_json(&replay)?;
        }
        TraceCommand::Stats {
            limit,
            include_replays,
        } => {
            let stats = app.trace_metrics(limit, include_replays)?;
            print_json(&stats)?;
        }
        TraceCommand::Snapshot {
            limit,
            include_replays,
        } => {
            let snapshot = app.create_trace_metrics_snapshot(limit, include_replays)?;
            print_json(&snapshot)?;
        }
        TraceCommand::Snapshots { limit } => {
            let snapshots = app.list_trace_metrics_snapshots(limit)?;
            print_json(&snapshots)?;
        }
        TraceCommand::Trend {
            limit,
            request_type,
        } => {
            let trend = app.trace_metrics_trend(limit, request_type.as_deref())?;
            print_json(&trend)?;
        }
        TraceCommand::Evidence {
            trace_limit,
            request_limit,
            enforce,
        } => {
            let report = app.collect_operability_evidence(trace_limit, request_limit)?;
            print_json(&report)?;
            if enforce && !report.passed {
                anyhow::bail!("operability evidence checks failed");
            }
        }
    }
    Ok(())
}

fn handle_eval(app: &AxiomMe, command: EvalCommand) -> Result<()> {
    match command {
        EvalCommand::Run {
            trace_limit,
            query_limit,
            search_limit,
            include_golden,
            golden_only,
        } => {
            let report = app.run_eval_loop_with_options(EvalRunOptions {
                trace_limit,
                query_limit,
                search_limit,
                include_golden,
                golden_only,
            })?;
            print_json(&report)?;
        }
        EvalCommand::Golden { command } => match command {
            EvalGoldenCommand::List => {
                let cases = app.list_eval_golden_queries()?;
                print_json(&cases)?;
            }
            EvalGoldenCommand::Add {
                query,
                target,
                expected_top,
            } => {
                let result =
                    app.add_eval_golden_query(&query, target.as_deref(), expected_top.as_deref())?;
                print_json(&result)?;
            }
            EvalGoldenCommand::MergeFromTraces {
                trace_limit,
                max_add,
            } => {
                let result = app.merge_eval_golden_from_traces(trace_limit, max_add)?;
                print_json(&result)?;
            }
        },
    }
    Ok(())
}

fn handle_benchmark(app: &AxiomMe, command: BenchmarkCommand) -> Result<()> {
    match command {
        BenchmarkCommand::Run {
            query_limit,
            search_limit,
            include_golden,
            include_trace,
            fixture_name,
        } => {
            let report = app.run_benchmark_suite(BenchmarkRunOptions {
                query_limit,
                search_limit,
                include_golden,
                include_trace,
                fixture_name,
            })?;
            print_json(&report)?;
        }
        BenchmarkCommand::List { limit } => {
            let reports = app.list_benchmark_reports(limit)?;
            print_json(&reports)?;
        }
        BenchmarkCommand::Trend { limit } => {
            let trend = app.benchmark_trend(limit)?;
            print_json(&trend)?;
        }
        BenchmarkCommand::Gate {
            threshold_p95_ms,
            min_top1_accuracy,
            gate_profile,
            max_p95_regression_pct,
            max_top1_regression_pct,
            window_size,
            required_passes,
            record,
            write_release_check,
            enforce,
        } => {
            let result = app.benchmark_gate_with_options(BenchmarkGateOptions {
                gate_profile,
                threshold_p95_ms,
                min_top1_accuracy,
                max_p95_regression_pct,
                max_top1_regression_pct,
                window_size,
                required_passes,
                record,
                write_release_check,
            })?;
            print_json(&result)?;
            if enforce && !result.passed {
                anyhow::bail!("benchmark gate failed");
            }
        }
        BenchmarkCommand::Fixture { command } => match command {
            BenchmarkFixtureCommand::Create {
                name,
                query_limit,
                include_golden,
                include_trace,
            } => {
                let summary = app.create_benchmark_fixture(
                    &name,
                    query_limit,
                    include_golden,
                    include_trace,
                )?;
                print_json(&summary)?;
            }
            BenchmarkFixtureCommand::List { limit } => {
                let fixtures = app.list_benchmark_fixtures(limit)?;
                print_json(&fixtures)?;
            }
        },
    }
    Ok(())
}

fn handle_security(app: &AxiomMe, command: SecurityCommand) -> Result<()> {
    match command {
        SecurityCommand::Audit {
            workspace_dir,
            enforce,
        } => {
            let report = app.run_security_audit(workspace_dir.as_deref())?;
            print_json(&report)?;
            if enforce && !report.passed {
                anyhow::bail!("security audit failed");
            }
        }
    }
    Ok(())
}

fn handle_release(app: &AxiomMe, command: ReleaseCommand) -> Result<()> {
    match command {
        ReleaseCommand::Pack {
            workspace_dir,
            replay_limit,
            replay_max_cycles,
            trace_limit,
            request_limit,
            eval_trace_limit,
            eval_query_limit,
            eval_search_limit,
            benchmark_query_limit,
            benchmark_search_limit,
            benchmark_threshold_p95_ms,
            benchmark_min_top1_accuracy,
            benchmark_max_p95_regression_pct,
            benchmark_max_top1_regression_pct,
            benchmark_window_size,
            benchmark_required_passes,
            enforce,
        } => {
            let report = app.collect_release_gate_pack(ReleaseGatePackOptions {
                workspace_dir,
                replay_limit,
                replay_max_cycles,
                trace_limit,
                request_limit,
                eval_trace_limit,
                eval_query_limit,
                eval_search_limit,
                benchmark_query_limit,
                benchmark_search_limit,
                benchmark_threshold_p95_ms,
                benchmark_min_top1_accuracy,
                benchmark_max_p95_regression_pct,
                benchmark_max_top1_regression_pct,
                benchmark_window_size,
                benchmark_required_passes,
            })?;
            print_json(&report)?;
            if enforce && !report.passed {
                anyhow::bail!("release gate pack failed");
            }
        }
    }
    Ok(())
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[derive(Debug, serde::Serialize, Default)]
struct QueueWorkReport {
    mode: String,
    iterations: u32,
    fetched: usize,
    processed: usize,
    done: usize,
    dead_letter: usize,
    requeued: usize,
    skipped: usize,
}

fn run_queue_worker(
    app: &AxiomMe,
    iterations: u32,
    limit: usize,
    sleep_ms: u64,
    include_dead_letter: bool,
    stop_when_idle: bool,
) -> Result<QueueWorkReport> {
    let mut total = QueueWorkReport::default();
    for i in 0..iterations {
        let report = app.replay_outbox(limit, include_dead_letter)?;
        total.iterations = i + 1;
        if total.mode.is_empty() {
            total.mode = "work".to_string();
        }
        total.fetched += report.fetched;
        total.processed += report.processed;
        total.done += report.done;
        total.dead_letter += report.dead_letter;
        total.requeued += report.requeued;
        total.skipped += report.skipped;

        if stop_when_idle && report.fetched == 0 {
            break;
        }
        if i + 1 < iterations {
            thread::sleep(Duration::from_millis(sleep_ms));
        }
    }
    Ok(total)
}

fn run_queue_daemon(
    app: &AxiomMe,
    max_cycles: u32,
    limit: usize,
    sleep_ms: u64,
    include_dead_letter: bool,
    stop_when_idle: bool,
    idle_cycles: u32,
) -> Result<QueueWorkReport> {
    let mut total = QueueWorkReport {
        mode: "daemon".to_string(),
        ..QueueWorkReport::default()
    };
    let mut idle_streak = 0u32;
    let mut cycle = 0u32;

    loop {
        if max_cycles > 0 && cycle >= max_cycles {
            break;
        }
        cycle += 1;

        let report = app.replay_outbox(limit, include_dead_letter)?;
        total.iterations = cycle;
        total.fetched += report.fetched;
        total.processed += report.processed;
        total.done += report.done;
        total.dead_letter += report.dead_letter;
        total.requeued += report.requeued;
        total.skipped += report.skipped;

        if report.fetched == 0 {
            idle_streak = idle_streak.saturating_add(1);
        } else {
            idle_streak = 0;
        }
        if stop_when_idle && idle_streak >= idle_cycles.max(1) {
            break;
        }

        thread::sleep(Duration::from_millis(sleep_ms));
    }

    Ok(total)
}

fn parse_scope_args(values: &[String]) -> Result<Option<Vec<Scope>>> {
    if values.is_empty() {
        return Ok(None);
    }

    let mut scopes = Vec::new();
    for raw in values {
        let scope = raw
            .parse::<Scope>()
            .map_err(|e| anyhow::anyhow!("invalid --scope value '{}': {}", raw, e))?;
        scopes.push(scope);
    }
    Ok(Some(scopes))
}

fn parse_search_budget(
    budget_ms: Option<u64>,
    budget_nodes: Option<usize>,
    budget_depth: Option<usize>,
) -> Option<SearchBudget> {
    if budget_ms.is_none() && budget_nodes.is_none() && budget_depth.is_none() {
        return None;
    }

    Some(SearchBudget {
        max_ms: budget_ms,
        max_nodes: budget_nodes,
        max_depth: budget_depth,
    })
}
