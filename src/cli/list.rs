use std::ffi::OsString;

use anyhow::Result;
use clap::Args;
use tabled::{
    Table, Tabled,
    settings::{Remove, Style, object::Rows},
};
use typed_path::{UnixPath, UnixPathBuf};

use crate::{
    assets::GAME_INDEX,
    ps2::{
        MemoryCard,
        fs::{DirEntry, Path as MemoryCardPath},
    },
};

#[derive(Tabled)]
struct TableRecord {
    name: String,
    title: String,
}

fn get_serial_from_file_name(file_name: &OsString) -> Option<String> {
    let file_name = file_name.clone().into_string().ok()?;

    if !file_name.starts_with("BA") {
        return None;
    }

    let substring = file_name.get(2..12)?;

    return Some(String::from(substring));
}

fn get_table_record_from_entry(path: &UnixPath, entry: DirEntry) -> Option<TableRecord> {
    let file_name = entry.file_name();

    let mut table_record = TableRecord {
        name: file_name.clone().into_string().ok()?,
        title: String::from(""),
    };

    if path != UnixPathBuf::from("/") {
        return Some(table_record);
    }

    let Some(serial) = get_serial_from_file_name(&file_name) else {
        return Some(table_record);
    };

    let Some(game_index_entry) = GAME_INDEX.get(&serial) else {
        return Some(table_record);
    };

    table_record.title = game_index_entry.name.clone();

    return Some(table_record);
}

#[derive(Args, Clone, Debug, PartialEq)]
pub struct ListCommand {
    path: MemoryCardPath,
}

impl ListCommand {
    pub fn run(self, memory_card: &mut MemoryCard) -> Result<()> {
        let path = self.path.get_path();

        let entries = memory_card.read_dir(path)?;

        let mut table_records: Vec<TableRecord> = Vec::new();

        for entry in entries {
            let entry = entry?;

            let Some(table_record) = get_table_record_from_entry(path, entry) else {
                continue;
            };

            table_records.push(table_record);
        }

        let mut table = Table::new(table_records);
        table.with(Remove::row(Rows::first()));
        table.with(Style::empty());

        println!("{}", table);

        Ok(())
    }
}
