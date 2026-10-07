use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::Args;

use crate::ps2::MemoryCard;

#[derive(Args, Clone, Debug, PartialEq)]
pub struct DumpCommand {
    /// Path to the memory card file.
    path: PathBuf,
    /// Destination folder for extracted files.
    dst: PathBuf,
}

impl DumpCommand {
    pub fn run(self) -> Result<()> {
        let bytes = fs::read(self.path).context("reading file")?;

        let memory_card = MemoryCard::from(bytes);

        memory_card
            .dump_to(&self.dst)
            .with_context(|| format!("dumping memory card to {}", self.dst.display()))?;

        Ok(())
    }
}
