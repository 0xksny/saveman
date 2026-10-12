use anyhow::Result;
use clap::Args;

use crate::ps2::{MemoryCard, fs::Path as MemoryCardPath};

#[derive(Args, Clone, Debug, PartialEq)]
pub struct RemoveCommand {
    path: MemoryCardPath,
}

impl RemoveCommand {
    pub fn run(self, memory_card: &mut MemoryCard) -> Result<()> {
        memory_card.remove_path(self.path.get_path())
    }
}
