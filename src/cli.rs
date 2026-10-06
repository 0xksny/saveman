use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::cli::read::ReadCommand;

mod read;

#[derive(Debug, Parser)]
#[command(name = "saveman", version)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            Commands::Read(command) => command.run(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Subcommand)]
enum Commands {
    /// Read a memory card file.
    Read(ReadCommand),
}
