use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::cli::delete::DeleteCommand;
use crate::cli::dump::DumpCommand;
use crate::cli::list::ListCommand;
use crate::cli::read::ReadCommand;

mod delete;
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
            Commands::Delete(command) => command.run(),
            Commands::Dump(command) => command.run(),
            Commands::List(command) => command.run(),
            Commands::Read(command) => command.run(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Subcommand)]
enum Commands {
    /// Delete a save file from a memory card.
    Delete(DeleteCommand),
    /// Dump the contents of a memory card.
    Dump(DumpCommand),
    /// List the contents of a memory card.
    List(ListCommand),
    /// Read a memory card.
    Read(ReadCommand),
}
