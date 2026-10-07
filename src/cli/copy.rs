use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::Args;

use crate::ps2::MemoryCard;

#[derive(Args, Clone, Debug, PartialEq)]
pub struct CopyCommand {
    /// Path to the source memory card file.
    src: PathBuf,
    /// Name of the save file to copy.
    name: String,
    /// Path to the destination memory card file.
    dst: PathBuf,
}

impl CopyCommand {
    pub fn run(self) -> Result<()> {
        let bytes_src = fs::read(&self.src).context("reading source file")?;
        let bytes_dst = fs::read(&self.dst).context("reading destination file")?;

        let memory_card_src = MemoryCard::from(bytes_src);
        let mut memory_card_dst = MemoryCard::from(bytes_dst);

        memory_card_dst
            .copy_save_folder_from(&memory_card_src, &self.name)
            .context("copying save folder")?;

        fs::write(&self.dst, Vec::<u8>::from(memory_card_dst))
            .context("writing memory card file")?;

        Ok(())
    }
}
