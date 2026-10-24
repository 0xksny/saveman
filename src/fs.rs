use std::path::Path;
use std::{fs, io};

pub trait FileSystem<Path: ?Sized> {
    type DirEntry;
    type ReadDir: Iterator<Item = io::Result<Self::DirEntry>>;

    fn create_dir<PathRef: AsRef<Path>>(&mut self, path: PathRef) -> io::Result<()>;
    fn read<PathRef: AsRef<Path>>(&self, path: PathRef) -> io::Result<Vec<u8>>;
    fn read_dir<PathRef: AsRef<Path>>(&self, path: PathRef) -> io::Result<Self::ReadDir>;
    fn write<PathRef: AsRef<Path>>(&mut self, path: PathRef, contents: &[u8]) -> io::Result<()>;
}

pub struct PhysicalFileSystem {}

impl PhysicalFileSystem {
    pub fn new() -> Self {
        PhysicalFileSystem {}
    }
}

impl FileSystem<Path> for PhysicalFileSystem {
    type DirEntry = fs::DirEntry;
    type ReadDir = fs::ReadDir;

    fn create_dir<PathRef: AsRef<Path>>(&mut self, path: PathRef) -> io::Result<()> {
        fs::create_dir(path)
    }

    fn read<PathRef: AsRef<Path>>(&self, path: PathRef) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn read_dir<PathRef: AsRef<Path>>(&self, path: PathRef) -> io::Result<Self::ReadDir> {
        fs::read_dir(path)
    }

    fn write<PathRef: AsRef<Path>>(&mut self, path: PathRef, contents: &[u8]) -> io::Result<()> {
        fs::write(path, contents)
    }
}
