use std::{collections::HashMap, sync::LazyLock};

use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
pub struct GameIndexEntry {
    pub name: String,
    pub region: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct GameIndex {
    entries: HashMap<String, GameIndexEntry>,
}

impl GameIndex {
    pub fn get(&self, serial: &str) -> Option<&GameIndexEntry> {
        return self.entries.get(serial);
    }
}

pub static GAME_INDEX: LazyLock<GameIndex> =
    LazyLock::new(|| serde_yaml::from_str(include_str!("../../assets/GameIndex.yaml")).unwrap());

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("SLUS-20946", &GameIndexEntry{name: String::from("Grand Theft Auto - San Andreas"), region: String::from("NTSC-U")})]
    fn game_index_get(#[case] serial: String, #[case] expected: &GameIndexEntry) {
        let actual = GAME_INDEX.get(&serial).unwrap();
        assert_eq!(actual, expected);
    }
}
