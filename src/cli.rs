use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::cli::dump::DumpCommand;
use crate::cli::list::ListCommand;
use crate::cli::read::ReadCommand;

mod dump;
mod list;
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
            Commands::Dump(command) => command.run(),
            Commands::List(command) => command.run(),
            Commands::Read(command) => command.run(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Subcommand)]
enum Commands {
    /// Dump the contents of a memory card.
    Dump(DumpCommand),
    /// List the contents of a memory card.
    List(ListCommand),
    /// Read a memory card.
    Read(ReadCommand),
}
