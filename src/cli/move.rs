use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::Args;

use crate::ps2::MemoryCard;

#[derive(Args, Clone, Debug, PartialEq)]
pub struct MoveCommand {
    /// Path to the source memory card file.
    src: PathBuf,
    /// Name of the save file to move.
    name: String,
    /// Path to the destination memory card file.
    dst: PathBuf,
}

impl MoveCommand {
    pub fn run(self) -> Result<()> {
        let bytes_src = fs::read(&self.src).context("reading source file")?;
        let bytes_dst = fs::read(&self.dst).context("reading destination file")?;

        let mut memory_card_src = MemoryCard::from(bytes_src);
        let mut memory_card_dst = MemoryCard::from(bytes_dst);

        memory_card_dst
            .copy_save_folder_from(&memory_card_src, &self.name)
            .context("copying save folder to destination")?;
        memory_card_src
            .delete_save_folder(&self.name)
            .context("deleting save folder from source")?;

        fs::write(&self.dst, Vec::<u8>::from(memory_card_dst))
            .context("writing destination memory card file")?;
        fs::write(&self.src, Vec::<u8>::from(memory_card_src))
            .context("writing source memory card file")?;

        Ok(())
    }
}
