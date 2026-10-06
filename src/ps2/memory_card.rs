use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path},
};

use anyhow::{Context, Result};

// https://www.psdevwiki.com/ps3/PS2_Savedata
const SUPERBLOCK_MAGIC_OFFSET: usize = 0x0000;
const SUPERBLOCK_MAGIC_LENGTH: usize = 0x001C;
const SUPERBLOCK_VERSION_OFFSET: usize = 0x001C;
const SUPERBLOCK_VERSION_LENGTH: usize = 0x000C;
const SUPERBLOCK_PAGE_LEN_OFFSET: usize = 0x0028;
const SUPERBLOCK_PAGE_LEN_LENGTH: usize = 0x0002;
const SUPERBLOCK_PAGES_PER_CLUSTER_OFFSET: usize = 0x002A;
const SUPERBLOCK_PAGES_PER_CLUSTER_LENGTH: usize = 0x0002;
const SUPERBLOCK_PAGES_PER_BLOCK_OFFSET: usize = 0x002C;
const SUPERBLOCK_PAGES_PER_BLOCK_LENGTH: usize = 0x0002;
const SUPERBLOCK_UNUSED_OFFSET: usize = 0x002E;
const SUPERBLOCK_UNUSED_LENGTH: usize = 0x0002;
const SUPERBLOCK_CLUSTERS_TOTAL_OFFSET: usize = 0x0030;
const SUPERBLOCK_CLUSTERS_TOTAL_LENGTH: usize = 0x0004;
const SUPERBLOCK_ALLOC_START_OFFSET: usize = 0x0034;
const SUPERBLOCK_ALLOC_START_LENGTH: usize = 0x0004;
const SUPERBLOCK_ALLOC_END_OFFSET: usize = 0x0038;
const SUPERBLOCK_ALLOC_END_LENGTH: usize = 0x0004;
const SUPERBLOCK_CLUSTER_ROOTDIR_OFFSET: usize = 0x003C;
const SUPERBLOCK_CLUSTER_ROOTDIR_LENGTH: usize = 0x0004;
const SUPERBLOCK_BBLOCK1_OFFSET: usize = 0x0040;
const SUPERBLOCK_BBLOCK1_LENGTH: usize = 0x0004;
const SUPERBLOCK_BBLOCK2_OFFSET: usize = 0x0044;
const SUPERBLOCK_BBLOCK2_LENGTH: usize = 0x0004;
const SUPERBLOCK_IND_FAT_TABLE_OFFSET: usize = 0x0050;
const SUPERBLOCK_IND_FAT_TABLE_LENGTH: usize = 0x0080;
const SUPERBLOCK_BAD_BLOCK_TABLE_OFFSET: usize = 0x00D0;
const SUPERBLOCK_BAD_BLOCK_TABLE_LENGTH: usize = 0x0080;
const SUPERBLOCK_CARD_TYPE_OFFSET: usize = 0x0150;
const SUPERBLOCK_CARD_TYPE_LENGTH: usize = 0x0001;
const SUPERBLOCK_CARD_FLAGS_OFFSET: usize = 0x0151;
const SUPERBLOCK_CARD_FLAGS_LENGTH: usize = 0x0001;
const SUPERBLOCK_RESERVED_OFFSET: usize = 0x0152;
const SUPERBLOCK_RESERVED_LENGTH: usize = 0x0002;
const SUPERBLOCK_UNKNOWN_OFFSET: usize = 0x0154;
const SUPERBLOCK_UNKNOWN_LENGTH: usize = 0x00BC;
const SUPERBLOCK_ECC_OFFSET: usize = 0x0200;
const SUPERBLOCK_ECC_LENGTH: usize = 0x0010;
const SUPERBLOCK_IFC_LIST_OFFSET: usize = 0x0050;
const SUPERBLOCK_IFC_LIST_ENTRIES: usize = 32;
const CLUSTER_LINK_END: u32 = u32::MAX;
const DIR_ENTRY_LENGTH: usize = 512;
const DIR_ENTRY_MODE_EXISTS: u16 = 0x8000;
const DIR_ENTRY_MODE_FILE: u16 = 0x0010;
const DIR_ENTRY_MODE_DIRECTORY: u16 = 0x0020;

// 0x000000	0x01C (28 bytes)	magic	Sony PS2 Memory Card Format	Memory Card identifyer
// 0x00001C	0x00C (12 bytes)	version	1.2.0.0	Memory Card format version. (1.2.0.0 = full support for bad_block_table map)
// 0x000028	0x002 (2 bytes)	page_len	512	Page size in bytes (without ECC)
// 0x00002A	0x002 (2 bytes)	pages_per_cluster	2	Number of pages in a cluster
// 0x00002C	0x002 (2 bytes)	pages_per_block	16	Number of pages in an block
// 0x00002E	0x002 (2 bytes)	not used	FF00
// 0x000030	0x004 (4 bytes)	clusters_total	8192	Total number of clusters
// 0x000034	0x004 (4 bytes)	alloc_start	41	First allocatable cluster number. Cluster values in the FAT and directory entries are relative to this
// 0x000038	0x004 (4 bytes)	alloc_end	8135	Cluster offset number after the highest allocatable cluster. Relative to alloc_start. Not used.
// 0x00003C	0x004 (4 bytes)	cluster_rootdir	0	Cluster offset of the first cluster of the root directory. Relative to alloc_start. Must be zero.
// 0x000040	0x004 (4 bytes)	bblock1	1023	Backup1 block number
// 0x000044	0x004 (4 bytes)	bblock2	1022	Backup2 block number
// 0x000050	0x080 (128 bytes)	ind_fat_table	8	Indirect FAT Table cluster number
// 0x0000D0	0x080 (128 bytes)	bad_block_table	-1	Bad blocks table (damaged blocks index)
// 0x000150	0x001 (1 byte)	card_type	2	Memory card type (2 = PS2 memory card)
// 000x0151	0x001 (1 byte)	card_flags	0x52	Memory Card features (0x01 = ECC support, 0x08 = Bad Block support, 0x10 = Erased state zeroed)
// 0x000152	0x002 (2 byte)	not used	FF
// 0x000154	0x0BC (188 bytes)	unknown
// 0x000200	0x010 (16 bytes)	ECC		Error Correction Code. The last 16 bytes of all the pages are reserved for this code. See explain below

pub struct MemoryCard {
    bytes: Vec<u8>,
}

impl MemoryCard {
    pub fn new(bytes: Vec<u8>) -> Self {
        MemoryCard { bytes }
    }

    fn get_bytes(&self, offset: usize, length: usize) -> Result<Vec<u8>> {
        let end = offset
            .checked_add(length)
            .context("memory card range overflow")?;
        Ok(self
            .bytes
            .get(offset..end)
            .context("memory card data is too short")?
            .to_vec())
    }

    fn get_u16(&self, offset: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(
            self.bytes
                .get(offset..offset + 2)
                .context("memory card data is too short")?
                .try_into()
                .context("reading 16-bit value")?,
        ))
    }

    fn get_u32(&self, offset: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.bytes
                .get(offset..offset + 4)
                .context("memory card data is too short")?
                .try_into()
                .context("reading 32-bit value")?,
        ))
    }

    fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(
            bytes
                .get(offset..offset + 4)
                .context("memory card structure is truncated")?
                .try_into()
                .context("reading 32-bit value")?,
        ))
    }

    fn geometry(&self) -> Result<(usize, usize, usize)> {
        let page_len = usize::from(self.get_page_len()?);
        let pages_per_cluster = usize::from(self.get_pages_per_cluster()?);
        let clusters_total = usize::try_from(self.get_clusters_total()?)?;
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

        anyhow::ensure!(
            indirect_index < SUPERBLOCK_IFC_LIST_ENTRIES,
            "cluster index exceeds the indirect FAT table"
        );
        let ifc_cluster = self.get_u32(SUPERBLOCK_IFC_LIST_OFFSET + indirect_index * 4)?;
        anyhow::ensure!(
            ifc_cluster != CLUSTER_LINK_END,
            "missing indirect FAT cluster"
        );
        let ifc_contents = self.read_cluster(ifc_cluster)?;
        let fat_cluster = Self::read_u32(&ifc_contents, indirect_entry_index * 4)?;
        anyhow::ensure!(fat_cluster != CLUSTER_LINK_END, "missing FAT cluster entry");
        let fat_contents = self.read_cluster(fat_cluster)?;
        Self::read_u32(&fat_contents, entry_index * 4)
    }

    fn cluster_chain(&self, first_cluster: u32) -> Result<Vec<u32>> {
        let alloc_start = self.get_alloc_start()?;
        let alloc_limit = self
            .get_clusters_total()?
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
        let alloc_start = self.get_alloc_start()?;
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
                let child_count = Self::read_u32(&contents, offset + 4)?;
                let child_cluster = Self::read_u32(&contents, offset + 0x10)?;
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
        let root_cluster = self.get_cluster_rootdir()?;
        let root_contents = self.read_cluster_chain(root_cluster)?;
        anyhow::ensure!(
            root_contents.len() >= DIR_ENTRY_LENGTH,
            "root directory is missing its directory entry"
        );
        let root_entry_count = Self::read_u32(&root_contents, 4)?;
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

            let file_length = usize::try_from(Self::read_u32(&contents, offset + 4)?)?;
            if file_length == 0 {
                return Ok(Some(Vec::new()));
            }
            let first_file_cluster = Self::read_u32(&contents, offset + 0x10)?;
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
        let root_cluster = self.get_cluster_rootdir()?;
        let root_contents = self.read_cluster_chain(root_cluster)?;
        anyhow::ensure!(
            root_contents.len() >= DIR_ENTRY_LENGTH,
            "root directory is missing its directory entry"
        );
        let root_entry_count = usize::try_from(Self::read_u32(&root_contents, 4)?)?;
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

            let directory_entry_count = Self::read_u32(&root_contents, offset + 4)?;
            let directory_cluster = Self::read_u32(&root_contents, offset + 0x10)?;
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
                let child_count = Self::read_u32(&contents, offset + 4)?;
                let child_cluster = Self::read_u32(&contents, offset + 0x10)?;
                self.dump_directory(
                    child_cluster,
                    child_count,
                    depth + 1,
                    &path,
                    visited_directories,
                )?;
            } else if mode & DIR_ENTRY_MODE_FILE != 0 {
                let file_length = usize::try_from(Self::read_u32(&contents, offset + 4)?)?;
                let first_file_cluster = Self::read_u32(&contents, offset + 0x10)?;
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
        let root_cluster = self.get_cluster_rootdir()?;
        let root_contents = self.read_cluster_chain(root_cluster)?;
        anyhow::ensure!(
            root_contents.len() >= DIR_ENTRY_LENGTH,
            "root directory is missing its directory entry"
        );
        let root_entry_count = Self::read_u32(&root_contents, 4)?;
        self.dump_directory(
            root_cluster,
            root_entry_count,
            0,
            destination,
            &mut Vec::new(),
        )
    }

    pub fn get_magic(&self) -> Result<String> {
        String::from_utf8(self.get_bytes(SUPERBLOCK_MAGIC_OFFSET, SUPERBLOCK_MAGIC_LENGTH)?)
            .context("creating string from utf8 bytes")
    }

    pub fn get_version(&self) -> Result<String> {
        String::from_utf8(self.get_bytes(SUPERBLOCK_VERSION_OFFSET, SUPERBLOCK_VERSION_LENGTH)?)
            .context("creating string from utf8 bytes")
    }

    pub fn get_page_len(&self) -> Result<u16> {
        self.get_u16(SUPERBLOCK_PAGE_LEN_OFFSET)
    }

    pub fn get_pages_per_cluster(&self) -> Result<u16> {
        self.get_u16(SUPERBLOCK_PAGES_PER_CLUSTER_OFFSET)
    }

    pub fn get_pages_per_block(&self) -> Result<u16> {
        self.get_u16(SUPERBLOCK_PAGES_PER_BLOCK_OFFSET)
    }

    pub fn get_unused(&self) -> Result<Vec<u8>> {
        self.get_bytes(SUPERBLOCK_UNUSED_OFFSET, SUPERBLOCK_UNUSED_LENGTH)
    }

    pub fn get_clusters_total(&self) -> Result<u32> {
        self.get_u32(SUPERBLOCK_CLUSTERS_TOTAL_OFFSET)
    }

    pub fn get_alloc_start(&self) -> Result<u32> {
        self.get_u32(SUPERBLOCK_ALLOC_START_OFFSET)
    }

    pub fn get_alloc_end(&self) -> Result<u32> {
        self.get_u32(SUPERBLOCK_ALLOC_END_OFFSET)
    }

    pub fn get_cluster_rootdir(&self) -> Result<u32> {
        self.get_u32(SUPERBLOCK_CLUSTER_ROOTDIR_OFFSET)
    }

    pub fn get_bblock1(&self) -> Result<u32> {
        self.get_u32(SUPERBLOCK_BBLOCK1_OFFSET)
    }

    pub fn get_bblock2(&self) -> Result<u32> {
        self.get_u32(SUPERBLOCK_BBLOCK2_OFFSET)
    }

    pub fn get_ind_fat_table(&self) -> Result<Vec<u8>> {
        self.get_bytes(
            SUPERBLOCK_IND_FAT_TABLE_OFFSET,
            SUPERBLOCK_IND_FAT_TABLE_LENGTH,
        )
    }

    pub fn get_bad_block_table(&self) -> Result<Vec<u8>> {
        self.get_bytes(
            SUPERBLOCK_BAD_BLOCK_TABLE_OFFSET,
            SUPERBLOCK_BAD_BLOCK_TABLE_LENGTH,
        )
    }

    pub fn get_card_type(&self) -> Result<u8> {
        Ok(*self
            .bytes
            .get(SUPERBLOCK_CARD_TYPE_OFFSET)
            .context("memory card data is too short")?)
    }

    pub fn get_card_flags(&self) -> Result<u8> {
        Ok(*self
            .bytes
            .get(SUPERBLOCK_CARD_FLAGS_OFFSET)
            .context("memory card data is too short")?)
    }

    pub fn get_reserved(&self) -> Result<Vec<u8>> {
        self.get_bytes(SUPERBLOCK_RESERVED_OFFSET, SUPERBLOCK_RESERVED_LENGTH)
    }

    pub fn get_unknown(&self) -> Result<Vec<u8>> {
        self.get_bytes(SUPERBLOCK_UNKNOWN_OFFSET, SUPERBLOCK_UNKNOWN_LENGTH)
    }

    pub fn get_ecc(&self) -> Result<Vec<u8>> {
        self.get_bytes(SUPERBLOCK_ECC_OFFSET, SUPERBLOCK_ECC_LENGTH)
    }
}
