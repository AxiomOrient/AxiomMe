use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct TraceArgs {
    #[command(subcommand)]
    pub command: TraceCommand,
}
#[derive(Debug, Subcommand)]
pub enum TraceCommand {
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
