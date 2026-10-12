use anyhow::Result;
use clap::Args;

use crate::ps2::MemoryCard;

#[derive(Args, Clone, Debug, PartialEq)]
pub struct CopyCommand {}

impl CopyCommand {
    pub fn run(self, memory_card: &mut MemoryCard) -> Result<()> {
        todo!();
    }
}
