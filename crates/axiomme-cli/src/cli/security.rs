use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SecurityAuditModeArg {
    Offline,
    Strict,
}

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
        #[arg(long, value_enum, default_value_t = SecurityAuditModeArg::Offline)]
        mode: SecurityAuditModeArg,
        #[arg(long, default_value_t = false)]
        enforce: bool,
    },
}
