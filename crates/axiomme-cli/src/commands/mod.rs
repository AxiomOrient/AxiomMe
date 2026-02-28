use std::path::Path;

use anyhow::{Context, Result};
use axiomme_core::AxiomMe;
use axiomme_core::markdown_preview::render_markdown_html as render_preview_html;
use axiomme_core::models::{
    AddResourceRequest, AddResourceWaitMode, MetadataFilter, ReconcileOptions, SearchRequest,
};

use crate::cli::{AddWaitModeArg, Commands, DocumentMode, QueueCommand};

mod handlers;
mod ontology;
mod queue;
mod support;
mod validation;
mod web;

use self::handlers::{
    handle_benchmark, handle_eval, handle_relation, handle_release, handle_security,
    handle_session, handle_trace,
};
use self::ontology::handle_ontology_command;
use self::queue::{run_queue_daemon, run_queue_worker};
use self::support::{
    build_add_ingest_options, parse_scope_args, parse_search_budget, print_json,
    read_document_content, read_preview_content,
};
use self::validation::{apply_bootstrap_mode, resolve_bootstrap_mode, validate_command_preflight};
use self::web::{WebServeOptions, serve};

pub(crate) fn run_from_root(root: &Path, command: Commands) -> Result<()> {
    validate_command_preflight(&command)?;

    if let Commands::Web(args) = &command {
        return run_web_handoff(root, &args.host, args.port);
    }

    let app = AxiomMe::new(root).context("failed to create app")?;
    run_validated(&app, root, command)
}

fn run_validated(app: &AxiomMe, root: &Path, command: Commands) -> Result<()> {
    if !matches!(&command, Commands::Web(_)) {
        let mode = resolve_bootstrap_mode(app, &command);
        apply_bootstrap_mode(app, mode)?;
    }

    match command {
        Commands::Init => {
            println!("initialized at {}", root.display());
        }
        Commands::Add(args) => {
            let ingest_options =
                build_add_ingest_options(args.markdown_only, args.include_hidden, &args.exclude)?;
            let mut request = AddResourceRequest::new(args.source.clone());
            request.target = args.target.clone();
            request.wait = args.wait;
            request.wait_mode = match args.wait_mode {
                AddWaitModeArg::Relaxed => AddResourceWaitMode::Relaxed,
                AddWaitModeArg::Strict => AddResourceWaitMode::Strict,
            };
            request.ingest_options = ingest_options;
            let result = app.add_resource_with_ingest_options(request)?;
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
                println!("{}", render_preview_html(&content));
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
                runtime_hints: Vec::new(),
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
        Commands::Ontology(args) => {
            handle_ontology_command(app, args.command)?;
        }
        Commands::Relation(args) => {
            handle_relation(app, args.command)?;
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
            run_web_handoff(root, &args.host, args.port)?;
        }
    }

    Ok(())
}

fn run_web_handoff(root: &Path, host: &str, port: u16) -> Result<()> {
    serve(root, WebServeOptions { host, port })
}

#[cfg(test)]
mod tests;
