mod cli;
mod commands;

use anyhow::{Context, Result};
use axiomme_core::AxiomMe;
use clap::Parser;

use crate::cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let app = AxiomMe::new(&cli.root).context("failed to create app")?;
    commands::run(&app, &cli.root, cli.command)
}
