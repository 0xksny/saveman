use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::Args;

use crate::ps2::MemoryCard;

#[derive(Args, Clone, Debug, PartialEq)]
pub struct NewCommand {
    /// Path to the memory card file.
    path: PathBuf,
}

impl NewCommand {
    pub fn run(self) -> Result<()> {
        let memory_card = MemoryCard::new();

        fs::write(&self.path, Vec::<u8>::from(memory_card)).context("writing memory card file")?;

        Ok(())
    }
}
