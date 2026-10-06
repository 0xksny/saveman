use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::Args;

use crate::ps2::MemoryCard;

#[derive(Args, Clone, Debug, PartialEq)]
pub struct ReadCommand {
    /// Path to the memory card file.
    path: PathBuf,
}

impl ReadCommand {
    pub fn run(self) -> Result<()> {
        let bytes = fs::read(self.path).context("reading file")?;

        let memory_card = MemoryCard::new(bytes);

        let magic = memory_card.get_magic().context("getting magic")?;

        print!("{magic}");

        Ok(())
    }
}
