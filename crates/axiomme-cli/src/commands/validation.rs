use anyhow::Result;
use axiomme_core::AxiomMe;

use crate::cli::{BenchmarkCommand, Commands, OntologyCommand, RelationCommand, ReleaseCommand};

use super::ontology::validate_ontology_action_input_source_selection;
use super::support::{
    parse_scope_args, validate_add_ingest_flags, validate_document_preview_source_selection,
    validate_document_save_source_selection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BootstrapMode {
    BootstrapOnly,
    PrepareRuntime,
}

pub(super) fn resolve_bootstrap_mode(app: &AxiomMe, command: &Commands) -> BootstrapMode {
    if command_needs_runtime_prepare(app, command) {
        BootstrapMode::PrepareRuntime
    } else {
        BootstrapMode::BootstrapOnly
    }
}

pub(super) fn apply_bootstrap_mode(app: &AxiomMe, mode: BootstrapMode) -> Result<()> {
    match mode {
        BootstrapMode::BootstrapOnly => {
            app.bootstrap()?;
            Ok(())
        }
        BootstrapMode::PrepareRuntime => {
            app.prepare_runtime()?;
            Ok(())
        }
    }
}

pub(super) const fn command_needs_runtime(command: &Commands) -> bool {
    match command {
        Commands::Abstract(_)
        | Commands::Overview(_)
        | Commands::Find(_)
        | Commands::Search(_)
        | Commands::Backend
        | Commands::Release(_) => true,
        Commands::Trace(args) => matches!(args.command, crate::cli::TraceCommand::Replay { .. }),
        Commands::Eval(args) => matches!(args.command, crate::cli::EvalCommand::Run { .. }),
        Commands::Benchmark(args) => matches!(
            args.command,
            crate::cli::BenchmarkCommand::Run { .. }
                | crate::cli::BenchmarkCommand::Amortized { .. }
        ),
        Commands::Web(_) => false,
        _ => false,
    }
}

pub(super) fn command_needs_runtime_prepare(app: &AxiomMe, command: &Commands) -> bool {
    if matches!(command, Commands::Search(_)) {
        return app.search_requires_runtime_prepare();
    }
    command_needs_runtime(command)
}

pub(super) fn validate_command_preflight(command: &Commands) -> Result<()> {
    match command {
        Commands::Add(args) => {
            validate_add_ingest_flags(args.markdown_only, args.include_hidden, &args.exclude)
        }
        Commands::Benchmark(args) => validate_benchmark_command(&args.command),
        Commands::Release(args) => validate_release_command(&args.command),
        Commands::Reconcile(args) => {
            let _ = parse_scope_args(&args.scopes)?;
            Ok(())
        }
        Commands::Document(args) => validate_document_command(&args.command),
        Commands::Ontology(args) => validate_ontology_command(&args.command),
        Commands::Relation(args) => validate_relation_command(&args.command),
        _ => Ok(()),
    }
}

fn validate_document_command(command: &crate::cli::DocumentCommand) -> Result<()> {
    match command {
        crate::cli::DocumentCommand::Load { .. } => Ok(()),
        crate::cli::DocumentCommand::Preview {
            uri,
            content,
            from,
            stdin,
        } => validate_document_preview_source_selection(
            uri.as_deref(),
            content.as_deref(),
            from.as_deref(),
            *stdin,
        ),
        crate::cli::DocumentCommand::Save {
            content,
            from,
            stdin,
            ..
        } => validate_document_save_source_selection(content.as_deref(), from.as_deref(), *stdin),
    }
}

fn validate_benchmark_command(command: &BenchmarkCommand) -> Result<()> {
    match command {
        BenchmarkCommand::Gate {
            window_size,
            required_passes,
            ..
        } => validate_gate_window_requirements(
            *window_size,
            *required_passes,
            "--window-size",
            "--required-passes",
        ),
        _ => Ok(()),
    }
}

fn validate_release_command(command: &ReleaseCommand) -> Result<()> {
    match command {
        ReleaseCommand::Pack {
            benchmark_window_size,
            benchmark_required_passes,
            ..
        } => validate_gate_window_requirements(
            *benchmark_window_size,
            *benchmark_required_passes,
            "--benchmark-window-size",
            "--benchmark-required-passes",
        ),
    }
}

fn validate_ontology_command(command: &OntologyCommand) -> Result<()> {
    match command {
        OntologyCommand::ActionValidate {
            input_json,
            input_file,
            input_stdin,
            ..
        }
        | OntologyCommand::ActionEnqueue {
            input_json,
            input_file,
            input_stdin,
            ..
        } => validate_ontology_action_input_source_selection(
            input_json.as_deref(),
            input_file.as_deref(),
            *input_stdin,
        ),
        _ => Ok(()),
    }
}

fn validate_relation_command(command: &RelationCommand) -> Result<()> {
    match command {
        RelationCommand::Link { uris, .. } => {
            if uris.len() < 2 {
                anyhow::bail!("relation link requires at least two --uri values");
            }
            Ok(())
        }
        RelationCommand::List { .. } | RelationCommand::Unlink { .. } => Ok(()),
    }
}

fn validate_gate_window_requirements(
    window_size: usize,
    required_passes: usize,
    window_flag: &str,
    required_flag: &str,
) -> Result<()> {
    if required_passes > window_size {
        anyhow::bail!(
            "{required_flag} ({required_passes}) cannot exceed {window_flag} ({window_size})"
        );
    }
    Ok(())
}
