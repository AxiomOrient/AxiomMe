use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Args)]
pub struct DocumentArgs {
    #[command(subcommand)]
    pub command: DocumentCommand,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DocumentMode {
    Document,
    Markdown,
}

#[derive(Debug, Subcommand)]
pub enum DocumentCommand {
    Load {
        uri: String,
        #[arg(long, value_enum, default_value_t = DocumentMode::Document)]
        mode: DocumentMode,
    },
    Preview {
        #[arg(long)]
        uri: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        content: Option<String>,
        #[arg(long)]
        from: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        stdin: bool,
    },
    Save {
        uri: String,
        #[arg(long, value_enum, default_value_t = DocumentMode::Document)]
        mode: DocumentMode,
        #[arg(long, allow_hyphen_values = true)]
        content: Option<String>,
        #[arg(long)]
        from: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        stdin: bool,
        #[arg(long)]
        expected_etag: Option<String>,
    },
}
