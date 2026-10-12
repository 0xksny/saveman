use std::ffi::OsString;

use typed_path::{UnixPath, UnixPathBuf};

use crate::ps2::fs::FileType;

#[derive(Clone, Debug, PartialEq)]
pub struct DirEntry {
    path: UnixPathBuf,
    file_type: FileType,
}

impl DirEntry {
    pub(crate) fn new(path: UnixPathBuf, file_type: FileType) -> Self {
        if !path.is_absolute() {
            panic!("path is not absolute");
        }

        if path != path.normalize() {
            panic!("path is not normalized");
        }

        if path == UnixPathBuf::from("/") {
            panic!("root directory can not be a directory entry");
        }

        Self { path, file_type }
    }

    pub fn path(&self) -> &UnixPath {
        &self.path
    }

    pub fn file_name(&self) -> OsString {
        OsString::from(String::from_utf8_lossy(self.path.file_name().unwrap()).into_owned())
    }

    pub fn file_type(&self) -> FileType {
        self.file_type
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn get_dir_entry_directory() -> DirEntry {
        DirEntry {
            path: UnixPathBuf::from("/foo/bar"),
            file_type: FileType::Directory,
        }
    }

    fn get_dir_entry_file() -> DirEntry {
        DirEntry {
            path: UnixPathBuf::from("/file.txt"),
            file_type: FileType::File,
        }
    }

    #[rstest]
    #[case::directory(
        UnixPathBuf::from("/foo/bar"),
        FileType::Directory,
        get_dir_entry_directory()
    )]
    #[case::file(UnixPathBuf::from("/file.txt"), FileType::File, get_dir_entry_file())]
    fn dir_entry_new_ok(
        #[case] path: UnixPathBuf,
        #[case] file_type: FileType,
        #[case] expected: DirEntry,
    ) {
        let actual = DirEntry::new(path, file_type);
        assert_eq!(actual, expected);
    }

    #[rstest]
    #[case::root(UnixPathBuf::from("/"), FileType::Directory)]
    #[case::relative_path(UnixPathBuf::from("foo/bar"), FileType::Directory)]
    #[case::unnormalized_path(UnixPathBuf::from("/foo/.//../bar"), FileType::Directory)]
    #[case::path_ends_with_parent_directory_component(
        UnixPathBuf::from("/foo/bar/.."),
        FileType::Directory
    )]
    #[should_panic]
    fn dir_entry_new_panic(#[case] path: UnixPathBuf, #[case] file_type: FileType) {
        DirEntry::new(path, file_type);
    }

    #[rstest]
    #[case::root(get_dir_entry_directory(), &UnixPathBuf::from("/foo/bar"))]
    #[case::root(get_dir_entry_file(), &UnixPathBuf::from("/file.txt"))]
    fn dir_entry_path(#[case] dir_entry: DirEntry, #[case] expected: &UnixPath) {
        let actual = dir_entry.path();
        assert_eq!(actual, expected)
    }

    #[rstest]
    #[case::root(get_dir_entry_directory(), OsString::from("bar"))]
    #[case::root(get_dir_entry_file(), OsString::from("file.txt"))]
    fn dir_entry_file_name(#[case] dir_entry: DirEntry, #[case] expected: OsString) {
        let actual = dir_entry.file_name();
        assert_eq!(actual, expected)
    }

    #[rstest]
    #[case::root(get_dir_entry_directory(), FileType::Directory)]
    #[case::root(get_dir_entry_file(), FileType::File)]
    fn dir_entry_file_type(#[case] dir_entry: DirEntry, #[case] expected: FileType) {
        let actual = dir_entry.file_type();
        assert_eq!(actual, expected)
    }
}
