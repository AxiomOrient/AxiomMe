use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: SessionCommand,
}
#[derive(Debug, Subcommand)]
pub enum SessionCommand {
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
