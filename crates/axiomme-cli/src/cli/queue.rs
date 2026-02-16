use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct QueueArgs {
    #[command(subcommand)]
    pub command: QueueCommand,
}
#[derive(Debug, Subcommand)]
pub enum QueueCommand {
    Status,
    Wait {
        #[arg(long)]
        timeout_secs: Option<u64>,
    },
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
