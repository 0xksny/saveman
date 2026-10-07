use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path},
};

use anyhow::{Context, Result};

const CLUSTER_LINK_END: u32 = u32::MAX;
const DIR_ENTRY_LENGTH: usize = 512;
const DIR_ENTRY_MODE_EXISTS: u16 = 0x8000;
const DIR_ENTRY_MODE_FILE: u16 = 0x0010;
const DIR_ENTRY_MODE_DIRECTORY: u16 = 0x0020;

fn get_bytes(bytes: &Vec<u8>, offset: usize, length: usize) -> Result<&[u8]> {
    bytes.get(offset..offset + length).context("getting bytes")
}

fn get_u8(bytes: &Vec<u8>, offset: usize) -> Result<u8> {
    Ok(u8::from_le_bytes(
        get_bytes(bytes, offset, 1)?
            .try_into()
            .context("reading 8-bit value")?,
    ))
}

fn get_u16(bytes: &Vec<u8>, offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        get_bytes(bytes, offset, 2)?
            .try_into()
            .context("reading 16-bit value")?,
    ))
}

fn get_u32(bytes: &Vec<u8>, offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        get_bytes(bytes, offset, 4)?
            .try_into()
            .context("reading 32-bit value")?,
    ))
}

pub struct MemoryCard {
    bytes: Vec<u8>,
}

impl MemoryCard {
    pub fn new(bytes: Vec<u8>) -> Self {
        MemoryCard { bytes }
    }

    fn get_bytes(&self, offset: usize, length: usize) -> Result<Vec<u8>> {
        Ok(get_bytes(&self.bytes, offset, length)?.to_vec())
    }

    fn get_u8(&self, offset: usize) -> Result<u8> {
        get_u8(&self.bytes, offset)
    }

    fn get_u16(&self, offset: usize) -> Result<u16> {
        get_u16(&self.bytes, offset)
    }

    fn get_u32(&self, offset: usize) -> Result<u32> {
        get_u32(&self.bytes, offset)
    }

    fn geometry(&self) -> Result<(usize, usize, usize)> {
        let page_len = usize::from(self.get_superblock_page_len()?);
        let pages_per_cluster = usize::from(self.get_superblock_pages_per_cluster()?);
        let clusters_total = usize::try_from(self.get_superblock_clusters_per_card()?)?;
        anyhow::ensure!(
            page_len == 512 || page_len == 1024,
            "unsupported page length: {page_len}"
        );
        anyhow::ensure!(
            (page_len == 512 && (pages_per_cluster == 1 || pages_per_cluster == 2))
                || (page_len == 1024 && pages_per_cluster == 1),
            "unsupported pages-per-cluster value: {pages_per_cluster}"
        );
        anyhow::ensure!(clusters_total > 0, "memory card declares no clusters");

        let data_cluster_len = page_len
            .checked_mul(pages_per_cluster)
            .context("cluster size overflow")?;
        let expected_data_len = clusters_total
            .checked_mul(data_cluster_len)
            .context("memory card size overflow")?;
        let raw_page_len = page_len.checked_add(16).context("raw page size overflow")?;
        let expected_raw_len = clusters_total
            .checked_mul(pages_per_cluster)
            .and_then(|pages| pages.checked_mul(raw_page_len))
            .context("raw memory card size overflow")?;

        let page_stride = if self.bytes.len() >= expected_raw_len {
            raw_page_len
        } else {
            anyhow::ensure!(
                self.bytes.len() >= expected_data_len,
                "memory card image is shorter than its declared geometry"
            );
            page_len
        };
        Ok((page_len, pages_per_cluster, page_stride))
    }

    fn read_cluster(&self, cluster: u32) -> Result<Vec<u8>> {
        let (page_len, pages_per_cluster, page_stride) = self.geometry()?;
        let first_page = usize::try_from(cluster)?
            .checked_mul(pages_per_cluster)
            .context("cluster page offset overflow")?;
        let mut contents = Vec::with_capacity(page_len * pages_per_cluster);
        for page in 0..pages_per_cluster {
            let page_index = first_page
                .checked_add(page)
                .context("page index overflow")?;
            let offset = page_index
                .checked_mul(page_stride)
                .context("page offset overflow")?;
            let end = offset
                .checked_add(page_len)
                .context("page range overflow")?;
            contents.extend_from_slice(
                self.bytes
                    .get(offset..end)
                    .with_context(|| {
                        format!(
                            "memory card page {page_index} at offset {offset} is truncated (image length {})",
                            self.bytes.len()
                        )
                    })?,
            );
        }
        Ok(contents)
    }

    fn fat_entry(&self, relative_cluster: u32) -> Result<u32> {
        let (page_len, pages_per_cluster, _) = self.geometry()?;
        let cluster_len = page_len
            .checked_mul(pages_per_cluster)
            .context("cluster size overflow")?;
        let entries_per_cluster = cluster_len / 4;
        anyhow::ensure!(
            entries_per_cluster > 0,
            "cluster is too small for FAT entries"
        );

        let fat_cluster_index = usize::try_from(relative_cluster)? / entries_per_cluster;
        let entry_index = usize::try_from(relative_cluster)? % entries_per_cluster;
        let indirect_index = fat_cluster_index / entries_per_cluster;
        let indirect_entry_index = fat_cluster_index % entries_per_cluster;

        let ifc_cluster = self.get_superblock_ifc_list_item(indirect_index)?;
        anyhow::ensure!(
            ifc_cluster != CLUSTER_LINK_END,
            "missing indirect FAT cluster"
        );
        let ifc_contents = self.read_cluster(ifc_cluster)?;
        let fat_cluster = get_u32(&ifc_contents, indirect_entry_index * 4)?;
        anyhow::ensure!(fat_cluster != CLUSTER_LINK_END, "missing FAT cluster entry");
        let fat_contents = self.read_cluster(fat_cluster)?;
        get_u32(&fat_contents, entry_index * 4)
    }

    fn cluster_chain(&self, first_cluster: u32) -> Result<Vec<u32>> {
        let alloc_start = self.get_superblock_alloc_offset()?;
        let alloc_limit = self
            .get_superblock_clusters_per_card()?
            .checked_sub(alloc_start)
            .context("allocation start is beyond the memory card")?;
        let mut chain = Vec::new();
        let mut current = first_cluster;

        while current != CLUSTER_LINK_END {
            anyhow::ensure!(
                current < alloc_limit,
                "cluster {current} is outside the allocatable range"
            );
            anyhow::ensure!(
                !chain.contains(&current),
                "cycle detected in cluster chain at {current}"
            );
            let physical_cluster = alloc_start
                .checked_add(current)
                .context("physical cluster number overflow")?;
            self.read_cluster(physical_cluster)?;
            chain.push(current);

            let next = self.fat_entry(current)?;
            if next == CLUSTER_LINK_END {
                break;
            }
            anyhow::ensure!(
                next & 0x8000_0000 != 0,
                "cluster {current} is marked free in the FAT"
            );
            current = next & 0x7FFF_FFFF;
        }
        Ok(chain)
    }

    fn read_cluster_chain(&self, first_cluster: u32) -> Result<Vec<u8>> {
        let alloc_start = self.get_superblock_alloc_offset()?;
        let chain = self.cluster_chain(first_cluster)?;
        let mut contents = Vec::new();
        for cluster in chain {
            let physical_cluster = alloc_start
                .checked_add(cluster)
                .context("physical cluster number overflow")?;
            contents.extend(self.read_cluster(physical_cluster)?);
        }
        Ok(contents)
    }

    fn append_directory_tree(
        &self,
        first_cluster: u32,
        entry_count: u32,
        depth: usize,
        visited_directories: &mut Vec<u32>,
        output: &mut String,
    ) -> Result<()> {
        anyhow::ensure!(depth <= 64, "directory nesting exceeds 64 levels");
        anyhow::ensure!(
            !visited_directories.contains(&first_cluster),
            "directory cycle detected at cluster {first_cluster}"
        );
        visited_directories.push(first_cluster);

        let contents = self.read_cluster_chain(first_cluster)?;
        let entry_count = usize::try_from(entry_count)?;
        anyhow::ensure!(
            entry_count <= contents.len() / DIR_ENTRY_LENGTH,
            "directory entry count exceeds its cluster chain"
        );

        for index in 0..entry_count {
            // The first entry describes the directory itself; child directories
            // also reserve the next entry for their parent link.
            if index == 0 || (depth > 0 && index == 1) {
                continue;
            }
            let offset = index * DIR_ENTRY_LENGTH;
            let mode = u16::from_le_bytes(
                contents[offset..offset + 2]
                    .try_into()
                    .context("reading directory entry mode")?,
            );
            if mode & DIR_ENTRY_MODE_EXISTS == 0 {
                continue;
            }

            let name_bytes = &contents[offset + 0x40..offset + 0x60];
            let name_end = name_bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(name_bytes.len());
            let name = String::from_utf8_lossy(&name_bytes[..name_end]);
            if name.is_empty() || name == "." || name == ".." {
                continue;
            }

            let is_directory = mode & DIR_ENTRY_MODE_DIRECTORY != 0;
            let prefix = "  ".repeat(depth);
            if is_directory {
                output.push_str(&format!("{prefix}{name}/\n"));
                let child_count = get_u32(&contents, offset + 4)?;
                let child_cluster = get_u32(&contents, offset + 0x10)?;
                self.append_directory_tree(
                    child_cluster,
                    child_count,
                    depth + 1,
                    visited_directories,
                    output,
                )?;
            } else if mode & DIR_ENTRY_MODE_FILE != 0 {
                output.push_str(&format!("{prefix}{name}\n"));
            }
        }

        visited_directories.pop();
        Ok(())
    }

    pub fn get_file_tree(&self) -> Result<String> {
        let root_cluster = self.get_superblock_rootdir_cluster()?;
        let root_contents = self.read_cluster_chain(root_cluster)?;
        anyhow::ensure!(
            root_contents.len() >= DIR_ENTRY_LENGTH,
            "root directory is missing its directory entry"
        );
        let root_entry_count = get_u32(&root_contents, 4)?;
        let mut tree = String::from("/\n");
        self.append_directory_tree(
            root_cluster,
            root_entry_count,
            0,
            &mut Vec::new(),
            &mut tree,
        )?;
        Ok(tree)
    }

    fn directory_file(
        &self,
        first_cluster: u32,
        entry_count: u32,
        file_name: &str,
    ) -> Result<Option<Vec<u8>>> {
        let contents = self.read_cluster_chain(first_cluster)?;
        let entry_count = usize::try_from(entry_count)?;
        anyhow::ensure!(
            entry_count <= contents.len() / DIR_ENTRY_LENGTH,
            "directory entry count exceeds its cluster chain"
        );

        for index in 2..entry_count {
            let offset = index * DIR_ENTRY_LENGTH;
            let mode = u16::from_le_bytes(
                contents[offset..offset + 2]
                    .try_into()
                    .context("reading directory entry mode")?,
            );
            if mode & (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_FILE)
                != (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_FILE)
            {
                continue;
            }

            let name_bytes = &contents[offset + 0x40..offset + 0x60];
            let name_end = name_bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(name_bytes.len());
            if !name_bytes[..name_end].eq_ignore_ascii_case(file_name.as_bytes()) {
                continue;
            }

            let file_length = usize::try_from(get_u32(&contents, offset + 4)?)?;
            if file_length == 0 {
                return Ok(Some(Vec::new()));
            }
            let first_file_cluster = get_u32(&contents, offset + 0x10)?;
            let mut file_contents = self.read_cluster_chain(first_file_cluster)?;
            anyhow::ensure!(
                file_length <= file_contents.len(),
                "file {file_name:?} is longer than its cluster chain"
            );
            file_contents.truncate(file_length);
            return Ok(Some(file_contents));
        }

        Ok(None)
    }

    pub fn get_save_icon_sys_files(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let root_cluster = self.get_superblock_rootdir_cluster()?;
        let root_contents = self.read_cluster_chain(root_cluster)?;
        anyhow::ensure!(
            root_contents.len() >= DIR_ENTRY_LENGTH,
            "root directory is missing its directory entry"
        );
        let root_entry_count = usize::try_from(get_u32(&root_contents, 4)?)?;
        anyhow::ensure!(
            root_entry_count <= root_contents.len() / DIR_ENTRY_LENGTH,
            "root directory entry count exceeds its cluster chain"
        );

        let mut saves = Vec::new();
        for index in 1..root_entry_count {
            let offset = index * DIR_ENTRY_LENGTH;
            let mode = u16::from_le_bytes(
                root_contents[offset..offset + 2]
                    .try_into()
                    .context("reading root directory entry mode")?,
            );
            if mode & (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
                != (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
            {
                continue;
            }

            let name_bytes = &root_contents[offset + 0x40..offset + 0x60];
            let name_end = name_bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(name_bytes.len());
            let name = String::from_utf8_lossy(&name_bytes[..name_end]).into_owned();
            if name.is_empty() || name == "." || name == ".." {
                continue;
            }

            let directory_entry_count = get_u32(&root_contents, offset + 4)?;
            let directory_cluster = get_u32(&root_contents, offset + 0x10)?;
            if let Some(icon_sys) =
                self.directory_file(directory_cluster, directory_entry_count, "icon.sys")?
            {
                saves.push((name, icon_sys));
            }
        }

        Ok(saves)
    }

    fn ensure_output_directory(path: &Path) -> Result<()> {
        match fs::symlink_metadata(path) {
            Ok(metadata) => anyhow::ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "dump destination is not a regular directory: {}",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(path)
                    .with_context(|| format!("creating directory {}", path.display()))?;
            }
            Err(error) => {
                return Err(error).with_context(|| format!("inspecting {}", path.display()));
            }
        }
        Ok(())
    }

    fn validate_entry_name(name: &str) -> Result<()> {
        anyhow::ensure!(
            !name.is_empty()
                && name != "."
                && name != ".."
                && !name
                    .chars()
                    .any(|character| matches!(character, '/' | '\\' | ':'))
                && !name.chars().any(char::is_control)
                && matches!(
                    Path::new(name).components().next(),
                    Some(Component::Normal(_))
                )
                && Path::new(name).components().count() == 1,
            "unsafe memory card entry name: {name:?}"
        );
        Ok(())
    }

    fn dump_directory(
        &self,
        first_cluster: u32,
        entry_count: u32,
        depth: usize,
        destination: &Path,
        visited_directories: &mut Vec<u32>,
    ) -> Result<()> {
        anyhow::ensure!(depth <= 64, "directory nesting exceeds 64 levels");
        anyhow::ensure!(
            !visited_directories.contains(&first_cluster),
            "directory cycle detected at cluster {first_cluster}"
        );
        visited_directories.push(first_cluster);

        let contents = self.read_cluster_chain(first_cluster)?;
        let entry_count = usize::try_from(entry_count)?;
        anyhow::ensure!(
            entry_count <= contents.len() / DIR_ENTRY_LENGTH,
            "directory entry count exceeds its cluster chain"
        );

        for index in 0..entry_count {
            if index == 0 || (depth > 0 && index == 1) {
                continue;
            }

            let offset = index * DIR_ENTRY_LENGTH;
            let mode = u16::from_le_bytes(
                contents[offset..offset + 2]
                    .try_into()
                    .context("reading directory entry mode")?,
            );
            if mode & DIR_ENTRY_MODE_EXISTS == 0 {
                continue;
            }

            let name_bytes = &contents[offset + 0x40..offset + 0x60];
            let name_end = name_bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(name_bytes.len());
            let name = String::from_utf8_lossy(&name_bytes[..name_end]);
            if name.is_empty() || name == "." || name == ".." {
                continue;
            }
            Self::validate_entry_name(&name)?;

            let path = destination.join(name.as_ref());
            if mode & DIR_ENTRY_MODE_DIRECTORY != 0 {
                Self::ensure_output_directory(&path)?;
                let child_count = get_u32(&contents, offset + 4)?;
                let child_cluster = get_u32(&contents, offset + 0x10)?;
                self.dump_directory(
                    child_cluster,
                    child_count,
                    depth + 1,
                    &path,
                    visited_directories,
                )?;
            } else if mode & DIR_ENTRY_MODE_FILE != 0 {
                let file_length = usize::try_from(get_u32(&contents, offset + 4)?)?;
                let first_file_cluster = get_u32(&contents, offset + 0x10)?;
                let mut file_contents = if file_length == 0 {
                    Vec::new()
                } else {
                    self.read_cluster_chain(first_file_cluster)?
                };
                anyhow::ensure!(
                    file_length <= file_contents.len(),
                    "file {name:?} is longer than its cluster chain"
                );
                file_contents.truncate(file_length);

                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .with_context(|| format!("creating file {}", path.display()))?;
                file.write_all(&file_contents)
                    .with_context(|| format!("writing file {}", path.display()))?;
            }
        }

        visited_directories.pop();
        Ok(())
    }

    pub fn dump_to(&self, destination: &Path) -> Result<()> {
        fs::create_dir_all(destination)
            .with_context(|| format!("creating dump destination {}", destination.display()))?;
        Self::ensure_output_directory(destination)?;
        let root_cluster = self.get_superblock_rootdir_cluster()?;
        let root_contents = self.read_cluster_chain(root_cluster)?;
        anyhow::ensure!(
            root_contents.len() >= DIR_ENTRY_LENGTH,
            "root directory is missing its directory entry"
        );
        let root_entry_count = get_u32(&root_contents, 4)?;
        self.dump_directory(
            root_cluster,
            root_entry_count,
            0,
            destination,
            &mut Vec::new(),
        )
    }

    pub fn get_superblock_magic(&self) -> Result<String> {
        String::from_utf8(self.get_bytes(0x0000, 0x001C)?)
            .context("creating string from utf8 bytes")
    }

    pub fn get_superblock_version(&self) -> Result<String> {
        String::from_utf8(self.get_bytes(0x001C, 0x000C)?)
            .context("creating string from utf8 bytes")
    }

    pub fn get_superblock_page_len(&self) -> Result<u16> {
        self.get_u16(0x0028)
    }

    pub fn get_superblock_pages_per_cluster(&self) -> Result<u16> {
        self.get_u16(0x002A)
    }

    pub fn get_superblock_pages_per_block(&self) -> Result<u16> {
        self.get_u16(0x002C)
    }

    pub fn get_superblock_clusters_per_card(&self) -> Result<u32> {
        self.get_u32(0x0030)
    }

    pub fn get_superblock_alloc_offset(&self) -> Result<u32> {
        self.get_u32(0x0034)
    }

    pub fn get_superblock_alloc_end(&self) -> Result<u32> {
        self.get_u32(0x0038)
    }

    pub fn get_superblock_rootdir_cluster(&self) -> Result<u32> {
        self.get_u32(0x003C)
    }

    pub fn get_superblock_backup_block1(&self) -> Result<u32> {
        self.get_u32(0x0040)
    }

    pub fn get_superblock_backup_block2(&self) -> Result<u32> {
        self.get_u32(0x0044)
    }

    pub fn get_superblock_ifc_list(&self) -> Result<Vec<u8>> {
        self.get_bytes(0x0050, 0x0080)
    }

    pub fn get_superblock_ifc_list_item(&self, index: usize) -> Result<u32> {
        anyhow::ensure!(index < 0x0080 / 4, "index exceeds list length");
        self.get_u32(0x0050 + index * 4)
    }

    pub fn get_superblock_bad_block_list(&self) -> Result<Vec<u8>> {
        self.get_bytes(0x00D0, 0x0080)
    }

    pub fn get_superblock_bad_block_list_item(&self, index: usize) -> Result<u32> {
        anyhow::ensure!(index < 0x0080 / 4, "index exceeds list length");
        self.get_u32(0x00D0 + index * 4)
    }

    pub fn get_superblock_card_type(&self) -> Result<u8> {
        self.get_u8(0x0150)
    }

    pub fn get_superblock_card_flags(&self) -> Result<u8> {
        self.get_u8(0x151)
    }
}
