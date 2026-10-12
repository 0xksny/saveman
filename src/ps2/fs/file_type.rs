#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileType {
    Directory,
    File,
}

impl FileType {
    pub fn is_dir(self) -> bool {
        matches!(self, Self::Directory)
    }

    pub fn is_file(self) -> bool {
        matches!(self, Self::File)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::is_false(FileType::File, false)]
    #[case::is_true(FileType::Directory, true)]
    fn file_entry_is_dir(#[case] file_type: FileType, #[case] expected: bool) {
        let actual = file_type.is_dir();
        assert_eq!(actual, expected);
    }

    #[rstest]
    #[case::is_false(FileType::Directory, false)]
    #[case::is_true(FileType::File, true)]
    fn file_entry_is_file(#[case] file_type: FileType, #[case] expected: bool) {
        let actual = file_type.is_file();
        assert_eq!(actual, expected);
    }
}
