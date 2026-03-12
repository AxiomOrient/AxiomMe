use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct EvalArgs {
    #[command(subcommand)]
    pub command: EvalCommand,
}
#[derive(Debug, Subcommand)]
pub enum EvalCommand {
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
pub enum EvalGoldenCommand {
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
