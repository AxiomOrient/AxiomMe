use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct SecurityArgs {
    #[command(subcommand)]
    pub command: SecurityCommand,
}
#[derive(Debug, Subcommand)]
pub enum SecurityCommand {
    Audit {
        #[arg(long)]
        workspace_dir: Option<String>,
        #[arg(long, default_value = "offline")]
        mode: String,
        #[arg(long, default_value_t = false)]
        enforce: bool,
    },
}
