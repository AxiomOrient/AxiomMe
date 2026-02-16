use std::io::Read;
use std::path::Path;
use std::{fs, io};

use anyhow::Result;
use axiomme_core::models::{
    AddResourceIngestOptions, MetadataFilter, ReconcileOptions, SearchBudget, SearchRequest,
};
use axiomme_core::{AxiomMe, Scope};
use axiomme_web::{render_markdown_preview, serve_web};

use crate::cli::{Commands, DocumentMode, QueueCommand};

mod handlers;
mod queue;

use self::handlers::{
    handle_benchmark, handle_eval, handle_release, handle_security, handle_session, handle_trace,
};
use self::queue::{run_queue_daemon, run_queue_worker};

#[expect(
    clippy::too_many_lines,
    reason = "explicit top-level CLI dispatch keeps command wiring easy to audit"
)]
pub fn run(app: &AxiomMe, root: &Path, command: Commands) -> Result<()> {
    if command_needs_runtime_prepare(app, &command) {
        app.prepare_runtime()?;
    } else {
        app.bootstrap()?;
    }

    match command {
        Commands::Init => {
            println!("initialized at {}", root.display());
        }
        Commands::Add(args) => {
            let ingest_options =
                build_add_ingest_options(args.markdown_only, args.include_hidden, &args.exclude)?;
            let result = app.add_resource_with_ingest_options(
                &args.source,
                args.target.as_deref(),
                None,
                None,
                args.wait,
                None,
                ingest_options,
            )?;
            print_json(&result)?;
        }
        Commands::Ls(args) => {
            let entries = app.ls(&args.uri, args.recursive, false)?;
            print_json(&entries)?;
        }
        Commands::Glob(args) => {
            let result = app.glob(&args.pattern, args.uri.as_deref())?;
            print_json(&result)?;
        }
        Commands::Read(args) => {
            println!("{}", app.read(&args.uri)?);
        }
        Commands::Abstract(args) => {
            println!("{}", app.abstract_text(&args.uri)?);
        }
        Commands::Overview(args) => {
            println!("{}", app.overview(&args.uri)?);
        }
        Commands::Mkdir(args) => {
            app.mkdir(&args.uri)?;
            print_json(&serde_json::json!({
                "status": "ok",
                "uri": args.uri,
            }))?;
        }
        Commands::Rm(args) => {
            app.rm(&args.uri, args.recursive)?;
            print_json(&serde_json::json!({
                "status": "ok",
                "uri": args.uri,
                "recursive": args.recursive,
            }))?;
        }
        Commands::Mv(args) => {
            app.mv(&args.from_uri, &args.to_uri)?;
            print_json(&serde_json::json!({
                "status": "ok",
                "from_uri": args.from_uri,
                "to_uri": args.to_uri,
            }))?;
        }
        Commands::Tree(args) => {
            let tree = app.tree(&args.uri)?;
            print_json(&tree)?;
        }
        Commands::Document(args) => match args.command {
            crate::cli::DocumentCommand::Load { uri, mode } => {
                let document = match mode {
                    DocumentMode::Document => app.load_document(&uri)?,
                    DocumentMode::Markdown => app.load_markdown(&uri)?,
                };
                print_json(&document)?;
            }
            crate::cli::DocumentCommand::Preview {
                uri,
                content,
                from,
                stdin,
            } => {
                let content = read_preview_content(app, uri, content, from, stdin)?;
                println!("{}", render_markdown_preview(&content));
            }
            crate::cli::DocumentCommand::Save {
                uri,
                mode,
                content,
                from,
                stdin,
                expected_etag,
            } => {
                let content = read_document_content(content, from, stdin)?;
                let saved = match mode {
                    DocumentMode::Document => {
                        app.save_document(&uri, &content, expected_etag.as_deref())?
                    }
                    DocumentMode::Markdown => {
                        app.save_markdown(&uri, &content, expected_etag.as_deref())?
                    }
                };
                print_json(&saved)?;
            }
        },
        Commands::Find(args) => {
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
            let budget = parse_search_budget(args.budget_ms, args.budget_nodes, args.budget_depth);
            let result = app.search_with_request(SearchRequest {
                query: args.query,
                target_uri: args.target,
                session: args.session,
                limit: Some(args.limit),
                score_threshold: args.score_threshold,
                min_match_tokens: args.min_match_tokens,
                filter: None::<MetadataFilter>,
                budget,
            })?;
            print_json(&result)?;
        }
        Commands::Backend => {
            let status = app.backend_status()?;
            print_json(&status)?;
        }
        Commands::Queue(args) => match args.command {
            QueueCommand::Status => {
                let overview = app.queue_overview()?;
                print_json(&overview)?;
            }
            QueueCommand::Wait { timeout_secs } => {
                app.wait_processed(timeout_secs)?;
                let overview = app.queue_overview()?;
                print_json(&overview)?;
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
                    app,
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
                    app,
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
            handle_trace(app, args.command)?;
        }
        Commands::Eval(args) => {
            handle_eval(app, args.command)?;
        }
        Commands::Benchmark(args) => {
            handle_benchmark(app, args.command)?;
        }
        Commands::Security(args) => {
            handle_security(app, args.command)?;
        }
        Commands::Release(args) => {
            handle_release(app, args.command)?;
        }
        Commands::Reconcile(args) => {
            let scopes = parse_scope_args(&args.scopes)?;
            let report = app.reconcile_state_with_options(&ReconcileOptions {
                dry_run: args.dry_run,
                scopes,
                max_drift_sample: args.max_drift_sample,
            })?;
            print_json(&report)?;
        }
        Commands::Session(args) => {
            handle_session(app, args.command)?;
        }
        Commands::ExportOvpack(args) => {
            let out = app.export_ovpack(&args.uri, &args.to)?;
            println!("{out}");
        }
        Commands::ImportOvpack(args) => {
            let out = app.import_ovpack(&args.file, &args.parent, args.force, args.vectorize)?;
            println!("{out}");
        }
        Commands::Web(args) => {
            serve_web(app.clone(), &args.host, args.port)?;
        }
    }

    Ok(())
}

const fn command_needs_runtime(command: &Commands) -> bool {
    match command {
        Commands::Abstract(_)
        | Commands::Overview(_)
        | Commands::Find(_)
        | Commands::Search(_)
        | Commands::Release(_)
        | Commands::Web(_) => true,
        Commands::Trace(args) => matches!(args.command, crate::cli::TraceCommand::Replay { .. }),
        Commands::Eval(args) => matches!(args.command, crate::cli::EvalCommand::Run { .. }),
        Commands::Benchmark(args) => matches!(
            args.command,
            crate::cli::BenchmarkCommand::Run { .. }
                | crate::cli::BenchmarkCommand::Amortized { .. }
        ),
        _ => false,
    }
}

fn command_needs_runtime_prepare(app: &AxiomMe, command: &Commands) -> bool {
    if matches!(command, Commands::Search(_)) {
        return app.search_requires_runtime_prepare();
    }
    command_needs_runtime(command)
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn parse_scope_args(values: &[String]) -> Result<Option<Vec<Scope>>> {
    if values.is_empty() {
        return Ok(None);
    }

    let mut scopes = Vec::new();
    for raw in values {
        let scope = raw
            .parse::<Scope>()
            .map_err(|e| anyhow::anyhow!("invalid --scope value '{raw}': {e}"))?;
        scopes.push(scope);
    }
    Ok(Some(scopes))
}

fn build_add_ingest_options(
    markdown_only: bool,
    include_hidden: bool,
    exclude: &[String],
) -> Result<AddResourceIngestOptions> {
    if include_hidden && !markdown_only {
        anyhow::bail!("--include-hidden requires --markdown-only");
    }
    if !exclude.is_empty() && !markdown_only {
        anyhow::bail!("--exclude requires --markdown-only");
    }

    if !markdown_only {
        return Ok(AddResourceIngestOptions::default());
    }

    let mut options = AddResourceIngestOptions::markdown_only_defaults();
    options.include_hidden = include_hidden;
    options.exclude_globs.extend(
        exclude
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
    );
    options.exclude_globs.sort();
    options.exclude_globs.dedup();
    Ok(options)
}

const fn parse_search_budget(
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

fn read_document_content(
    inline: Option<String>,
    from: Option<std::path::PathBuf>,
    stdin: bool,
) -> Result<String> {
    let mut selected = 0u8;
    if inline.is_some() {
        selected += 1;
    }
    if from.is_some() {
        selected += 1;
    }
    if stdin {
        selected += 1;
    }

    if selected == 0 {
        anyhow::bail!(
            "document save content source is required: use one of --content, --from <path>, --stdin"
        );
    }
    if selected > 1 {
        anyhow::bail!(
            "document save accepts exactly one content source: choose one of --content, --from, --stdin"
        );
    }

    if let Some(content) = inline {
        return Ok(content);
    }
    if let Some(path) = from {
        return Ok(fs::read_to_string(path)?);
    }

    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    Ok(buffer)
}

fn read_preview_content(
    app: &AxiomMe,
    uri: Option<String>,
    inline: Option<String>,
    from: Option<std::path::PathBuf>,
    stdin: bool,
) -> Result<String> {
    let mut selected = 0u8;
    if uri.is_some() {
        selected += 1;
    }
    if inline.is_some() {
        selected += 1;
    }
    if from.is_some() {
        selected += 1;
    }
    if stdin {
        selected += 1;
    }

    if selected == 0 {
        anyhow::bail!(
            "document preview source is required: use one of --uri, --content, --from <path>, --stdin"
        );
    }
    if selected > 1 {
        anyhow::bail!(
            "document preview accepts exactly one source: choose one of --uri, --content, --from, --stdin"
        );
    }

    if let Some(uri) = uri {
        let document = app.load_markdown(&uri)?;
        return Ok(document.content);
    }
    if let Some(content) = inline {
        return Ok(content);
    }
    if let Some(path) = from {
        return Ok(fs::read_to_string(path)?);
    }

    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    Ok(buffer)
}

#[cfg(test)]
mod tests;
