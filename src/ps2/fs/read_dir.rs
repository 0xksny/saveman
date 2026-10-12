use crate::ps2::fs::DirEntry;

pub struct ReadDir {
    entries: std::vec::IntoIter<DirEntry>,
}

impl ReadDir {
    pub(crate) fn new(entries: Vec<DirEntry>) -> Self {
        Self {
            entries: entries.into_iter(),
        }
    }
}

impl Iterator for ReadDir {
    type Item = std::io::Result<DirEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next().map(Ok)
    }
}

#[cfg(test)]
mod tests {
    use typed_path::UnixPathBuf;

    use crate::ps2::fs::FileType;

    use super::*;

    #[test]
    fn read_dir_iter() {
        let entries = vec![
            DirEntry::new(UnixPathBuf::from("/foo"), FileType::Directory),
            DirEntry::new(UnixPathBuf::from("/bar"), FileType::Directory),
            DirEntry::new(UnixPathBuf::from("/baz"), FileType::Directory),
        ];

        let read_dir = ReadDir::new(entries.clone());

        let mut i = 0;
        for entry in read_dir {
            let entry = entry.unwrap();

            assert_eq!(entry, entries[i]);

            i = i + 1;
        }
    }
}
