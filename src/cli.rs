use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::cli::copy::CopyCommand;
use crate::cli::list::ListCommand;
use crate::cli::r#move::MoveCommand;
use crate::cli::remove::RemoveCommand;
use crate::ps2::{MemoryCard, fs::Path as MemoryCardPath};

mod copy;
mod list;
mod r#move;
mod remove;

#[derive(Clone, Debug, PartialEq)]
pub enum CliPath {
    FileSystemPath(PathBuf),
    MemoryCardPath(MemoryCardPath),
}

impl FromStr for CliPath {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match MemoryCardPath::try_from(value) {
            Ok(memory_card_path) => Ok(CliPath::MemoryCardPath(memory_card_path)),
            Err(_) => Ok(CliPath::FileSystemPath(PathBuf::from(value))),
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "saveman", version)]
pub struct Cli {
    /// Path to the memory card file.
    memory_card_file: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let bytes = fs::read(&self.memory_card_file).context("reading memory card file")?;

        let mut memory_card = MemoryCard::from(bytes);

        match self.command {
            Commands::Copy(command) => command.run(&mut memory_card),
            Commands::List(command) => command.run(&mut memory_card),
            Commands::Move(command) => command.run(&mut memory_card),
            Commands::Remove(command) => command.run(&mut memory_card),
        }?;

        fs::write(&self.memory_card_file, memory_card.get_all_bytes())
            .context("writing memory card file")?;

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Subcommand)]
enum Commands {
    Copy(CopyCommand),
    List(ListCommand),
    Move(MoveCommand),
    Remove(RemoveCommand),
}
