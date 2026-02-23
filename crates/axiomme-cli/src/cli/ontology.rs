use clap::{Args, Subcommand};
use std::path::PathBuf;

fn parse_positive_usize(raw: &str) -> Result<usize, String> {
    let parsed = raw
        .parse::<usize>()
        .map_err(|_| format!("invalid integer value: {raw}"))?;
    if parsed == 0 {
        return Err("value must be >= 1".to_string());
    }
    Ok(parsed)
}

#[derive(Debug, Args)]
pub struct OntologyArgs {
    #[command(subcommand)]
    pub command: OntologyCommand,
}

#[derive(Debug, Subcommand)]
pub enum OntologyCommand {
    Validate {
        #[arg(long)]
        uri: Option<String>,
    },
    Pressure {
        #[arg(long)]
        uri: Option<String>,
        #[arg(long, default_value_t = 3)]
        min_action_types: usize,
        #[arg(long, default_value_t = 3)]
        min_invariants: usize,
        #[arg(long, default_value_t = 5)]
        min_action_invariant_total: usize,
        #[arg(long, default_value_t = 15_000)]
        min_link_types_per_object_basis_points: u32,
    },
    Trend {
        #[arg(long)]
        history_dir: PathBuf,
        #[arg(long, default_value_t = 3, value_parser = parse_positive_usize)]
        min_samples: usize,
        #[arg(long, default_value_t = 3, value_parser = parse_positive_usize)]
        consecutive_v2_candidate: usize,
    },
    ActionValidate {
        #[arg(long)]
        uri: Option<String>,
        #[arg(long)]
        action_id: String,
        #[arg(long)]
        queue_event_type: String,
        #[arg(long)]
        input_json: Option<String>,
        #[arg(long)]
        input_file: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        input_stdin: bool,
    },
    ActionEnqueue {
        #[arg(long)]
        uri: Option<String>,
        #[arg(long)]
        target_uri: String,
        #[arg(long)]
        action_id: String,
        #[arg(long)]
        queue_event_type: String,
        #[arg(long)]
        input_json: Option<String>,
        #[arg(long)]
        input_file: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        input_stdin: bool,
    },
    InvariantCheck {
        #[arg(long)]
        uri: Option<String>,
        #[arg(long, default_value_t = false)]
        enforce: bool,
    },
}
