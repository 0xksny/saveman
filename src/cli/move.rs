use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::Args;
use typed_path::UnixPath;

use crate::cli::CliPath;
use crate::ps2::{MemoryCard, fs::Path as MemoryCardPath};

#[derive(Args, Clone, Debug, PartialEq)]
pub struct MoveCommand {
    src: CliPath,
    dst: CliPath,
}

fn is_fs_to_fs<'a>(src: &'a CliPath, dst: &'a CliPath) -> Option<(&'a PathBuf, &'a PathBuf)> {
    if let CliPath::FileSystemPath(src) = src
        && let CliPath::FileSystemPath(dst) = dst
    {
        return Some((&src, &dst));
    }
    None
}

fn is_mc_to_fs<'a>(
    src: &'a CliPath,
    dst: &'a CliPath,
) -> Option<(&'a MemoryCardPath, &'a PathBuf)> {
    if let CliPath::MemoryCardPath(src) = src
        && let CliPath::FileSystemPath(dst) = dst
    {
        return Some((src, dst));
    }
    None
}

fn is_fs_to_mc<'a>(
    src: &'a CliPath,
    dst: &'a CliPath,
) -> Option<(&'a PathBuf, &'a MemoryCardPath)> {
    if let CliPath::FileSystemPath(src) = src
        && let CliPath::MemoryCardPath(dst) = dst
    {
        return Some((src, dst));
    }
    None
}

fn is_mc_to_mc<'a>(
    src: &'a CliPath,
    dst: &'a CliPath,
) -> Option<(&'a MemoryCardPath, &'a MemoryCardPath)> {
    if let CliPath::MemoryCardPath(src) = src
        && let CliPath::MemoryCardPath(dst) = dst
    {
        return Some((src, dst));
    }
    None
}

fn walk_mc_to_fs(memory_card: &mut MemoryCard, src: &UnixPath, dst: &Path) -> Result<()> {
    for entry in memory_card.read_dir(src)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_type = entry.file_type();

        if file_type.is_dir() {
            let src = src.join(&file_name);
            let dst = dst.join(&file_name);
            fs::create_dir(&dst)?;
            walk_mc_to_fs(memory_card, &src, &dst)?;
        } else if file_type.is_file() {
            let path = dst.join(&file_name);
            let contents: Vec<u8> = memory_card
                .read(entry.path())
                .context("reading entry at {path}")?;
            fs::write(&path, &contents)?;
        }
    }

    todo!("delete from src");

    Ok(())
}

fn walk_fs_to_mc(memory_card: &mut MemoryCard, src: &Path, dst: &UnixPath) -> Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            let src = src.join(&file_name);
            let dst = dst.join(&file_name);
            memory_card.create_dir(&dst)?;
            walk_fs_to_mc(memory_card, &src, &dst)?;
        } else if file_type.is_file() {
            let path = dst.join(&file_name);
            let contents = fs::read(entry.path()).context("reading entry at {path}")?;
            memory_card.write(&path, &contents)?;
        }
    }

    todo!("delete from src");

    Ok(())
}

fn walk_mc_to_mc(memory_card: &mut MemoryCard, src: &UnixPath, dst: &UnixPath) -> Result<()> {
    for entry in memory_card.read_dir(src)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_type = entry.file_type();

        if file_type.is_dir() {
            let src = src.join(&file_name);
            let dst = dst.join(&file_name);
            memory_card.create_dir(&dst)?;
            walk_mc_to_mc(memory_card, &src, &dst)?;
        } else if file_type.is_file() {
            let path = dst.join(&file_name);
            let contents = memory_card
                .read(entry.path())
                .context("reading entry at {path}")?;
            memory_card.write(&path, &contents)?;
        }
    }

    todo!("delete from src");

    Ok(())
}

impl MoveCommand {
    pub fn run(self, memory_card: &mut MemoryCard) -> Result<()> {
        if let Some((_, _)) = is_fs_to_fs(&self.src, &self.dst) {
            return Err(anyhow!(
                "at least one of src or dst must be a memory card path"
            ));
        }

        if let Some((src, dst)) = is_mc_to_fs(&self.src, &self.dst) {
            walk_mc_to_fs(memory_card, src.get_path(), dst)?;
            return Ok(());
        }

        if let Some((src, dst)) = is_fs_to_mc(&self.src, &self.dst) {
            walk_fs_to_mc(memory_card, src, dst.get_path())?;
            return Ok(());
        }

        if let Some((src, dst)) = is_mc_to_mc(&self.src, &self.dst) {
            walk_mc_to_mc(memory_card, src.get_path(), dst.get_path())?;
            return Ok(());
        }

        unreachable!();
    }
}
