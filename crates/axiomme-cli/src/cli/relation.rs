use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct RelationArgs {
    #[command(subcommand)]
    pub command: RelationCommand,
}

#[derive(Debug, Subcommand)]
pub enum RelationCommand {
    List {
        #[arg(long)]
        owner_uri: String,
    },
    Link {
        #[arg(long)]
        owner_uri: String,
        #[arg(long)]
        relation_id: String,
        #[arg(long = "uri", required = true)]
        uris: Vec<String>,
        #[arg(long)]
        reason: String,
    },
    Unlink {
        #[arg(long)]
        owner_uri: String,
        #[arg(long)]
        relation_id: String,
    },
}
