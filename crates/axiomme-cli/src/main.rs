mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;

use crate::cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    commands::run_from_root(&cli.root, cli.command)
}
