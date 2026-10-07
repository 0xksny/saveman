use std::{fs, path::PathBuf};

use anyhow::{Context, Result, ensure};
use clap::Args;
use tabled::{builder::Builder, settings::Style};

use crate::ps2::{IconSys, MemoryCard};

#[derive(Args, Clone, Debug, PartialEq)]
pub struct ListCommand {
    /// Path to the memory card file.
    path: PathBuf,
}

impl ListCommand {
    pub fn run(self) -> Result<()> {
        let bytes = fs::read(self.path).context("reading file")?;

        let memory_card = MemoryCard::from(bytes);

        let mut builder = Builder::new();
        builder.push_record(["Save folder", "Title"]);

        for (folder, bytes) in memory_card
            .get_save_icon_sys_files()
            .context("getting save icon.sys files")?
        {
            let icon_sys = IconSys::new(bytes);
            let magic = icon_sys
                .get_magic()
                .with_context(|| format!("reading icon.sys magic for {folder}"))?;
            ensure!(magic == "PS2D", "invalid icon.sys magic in {folder}");

            let title = icon_sys
                .get_title()
                .with_context(|| format!("reading save title for {folder}"))?;
            builder.push_record([folder, title]);
        }

        println!("{}", builder.build().with(Style::rounded()));

        Ok(())
    }
}
