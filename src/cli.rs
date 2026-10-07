use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::cli::copy::CopyCommand;
use crate::cli::delete::DeleteCommand;
use crate::cli::dump::DumpCommand;
use crate::cli::list::ListCommand;
use crate::cli::r#move::MoveCommand;
use crate::cli::new::NewCommand;
use crate::cli::read::ReadCommand;

mod copy;
mod delete;
mod dump;
mod list;
mod r#move;
mod new;
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
            Commands::Copy(command) => command.run(),
            Commands::Delete(command) => command.run(),
            Commands::Dump(command) => command.run(),
            Commands::List(command) => command.run(),
            Commands::Move(command) => command.run(),
            Commands::New(command) => command.run(),
            Commands::Read(command) => command.run(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Subcommand)]
enum Commands {
    /// Copy a save file between memory cards.
    Copy(CopyCommand),
    /// Delete a save file from a memory card.
    Delete(DeleteCommand),
    /// Dump the contents of a memory card.
    Dump(DumpCommand),
    /// List the contents of a memory card.
    List(ListCommand),
    /// Move a save file between memory cards.
    Move(MoveCommand),
    /// Create a new memory card.
    New(NewCommand),
    /// Read a memory card.
    Read(ReadCommand),
}
