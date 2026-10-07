use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::Args;

use crate::ps2::MemoryCard;

#[derive(Args, Clone, Debug, PartialEq)]
pub struct DeleteCommand {
    /// Path to the memory card file.
    path: PathBuf,
    /// Name of the save file to delete.
    name: String,
}

impl DeleteCommand {
    pub fn run(self) -> Result<()> {
        let bytes = fs::read(&self.path).context("reading file")?;

        let mut memory_card = MemoryCard::from(bytes);

        memory_card
            .delete_save_folder(&self.name)
            .context("deleting save folder")?;

        fs::write(&self.path, Vec::<u8>::from(memory_card)).context("writing memory card file")?;

        Ok(())
    }
}
