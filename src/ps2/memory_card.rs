use std::{
    io,
    path::{Component, Path},
};

use anyhow::{Context, Result, anyhow};
use typed_path::{UnixComponent, UnixPath};

use crate::{
    fs::FileSystem,
    ps2::fs,
    ps2::fs::{DirEntry, FileType},
};

const CLUSTER_LINK_END: u32 = u32::MAX;
const DIR_ENTRY_LENGTH: usize = 512;
const DIR_ENTRY_MODE_EXISTS: u16 = 0x8000;
const DIR_ENTRY_MODE_FILE: u16 = 0x0010;
const DIR_ENTRY_MODE_DIRECTORY: u16 = 0x0020;
const PAGE_LEN: usize = 512;
const PAGE_SPARE_LEN: usize = 16;
const CLUSTERS_PER_CARD: usize = 8192;
const PAGES_PER_CLUSTER: usize = 2;
const ALLOC_OFFSET: usize = 41;

fn set_bytes(bytes: &mut Vec<u8>, offset: usize, value: &[u8]) {
    bytes[offset..offset + value.len()].copy_from_slice(value);
}

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

fn calculate_ecc(chunk: &[u8]) -> [u8; 3] {
    let mut column = 0x77u8;
    let mut line_complement = 0x7Fu8;
    let mut line = 0x7Fu8;
    for (index, byte) in chunk.iter().copied().enumerate() {
        for (bit, mask) in [0x55u8, 0x33, 0x0F, 0x00, 0xAA, 0xCC, 0xF0]
            .into_iter()
            .enumerate()
        {
            column ^= (((byte & mask).count_ones() & 1) as u8) << bit;
        }
        if byte.count_ones() & 1 != 0 {
            let index = index as u8;
            line_complement ^= !index;
            line ^= index;
        }
    }
    [column, line_complement & 0x7F, line & 0x7F]
}

fn write_page_data(
    bytes: &mut [u8],
    page_start: usize,
    page_len: usize,
    in_page: usize,
    contents: &[u8],
) -> Result<()> {
    anyhow::ensure!(
        in_page
            .checked_add(contents.len())
            .context("page write range overflow")?
            <= page_len,
        "page write exceeds data area"
    );
    anyhow::ensure!(
        page_len % 128 == 0 && page_len / 128 * 3 <= PAGE_SPARE_LEN,
        "unsupported page length for ECC: {page_len}"
    );
    let data_end = page_start
        .checked_add(page_len)
        .context("page data range overflow")?;
    let spare_end = data_end
        .checked_add(PAGE_SPARE_LEN)
        .context("page spare range overflow")?;
    anyhow::ensure!(
        spare_end <= bytes.len(),
        "memory card page exceeds image length"
    );
    let write_start = page_start
        .checked_add(in_page)
        .context("page write offset overflow")?;
    let write_end = write_start
        .checked_add(contents.len())
        .context("page write range overflow")?;
    bytes[write_start..write_end].copy_from_slice(contents);

    for chunk_index in 0..page_len / 128 {
        let chunk_start = page_start + chunk_index * 128;
        let ecc = calculate_ecc(&bytes[chunk_start..chunk_start + 128]);
        let spare_start = data_end + chunk_index * 3;
        bytes[spare_start..spare_start + 3].copy_from_slice(&ecc);
    }
    Ok(())
}

pub struct MemoryCard {
    bytes: Vec<u8>,
}

impl From<Vec<u8>> for MemoryCard {
    fn from(bytes: Vec<u8>) -> Self {
        MemoryCard { bytes }
    }
}

impl From<MemoryCard> for Vec<u8> {
    fn from(memory_card: MemoryCard) -> Self {
        memory_card.bytes
    }
}

impl MemoryCard {
    pub fn new() -> Self {
        let data_len = CLUSTERS_PER_CARD * PAGES_PER_CLUSTER * PAGE_LEN;
        let mut data = vec![0; data_len];

        set_bytes(&mut data, 0x0000, b"Sony PS2 Memory Card Format ");
        set_bytes(&mut data, 0x001C, b"1.2.0.0\0\0\0\0\0");
        set_bytes(&mut data, 0x0028, &512u16.to_le_bytes());
        set_bytes(&mut data, 0x002A, &2u16.to_le_bytes());
        set_bytes(&mut data, 0x002C, &16u16.to_le_bytes());
        set_bytes(&mut data, 0x002E, &0xFF00u16.to_le_bytes());
        set_bytes(&mut data, 0x0030, &(CLUSTERS_PER_CARD as u32).to_le_bytes());
        set_bytes(&mut data, 0x0034, &(ALLOC_OFFSET as u32).to_le_bytes());
        set_bytes(&mut data, 0x0038, &8135u32.to_le_bytes());
        set_bytes(&mut data, 0x003C, &0u32.to_le_bytes());
        set_bytes(&mut data, 0x0040, &1023u32.to_le_bytes());
        set_bytes(&mut data, 0x0044, &1022u32.to_le_bytes());
        // The IFC cluster is at absolute cluster 8; its first entry points
        // to the first FAT cluster at absolute cluster 9.
        set_bytes(&mut data, 0x0050, &8u32.to_le_bytes());
        data[0x0054..0x00D0].fill(0xFF);
        data[0x00D0..0x0150].fill(0xFF);
        set_bytes(&mut data, 0x0150, &[2, 0x52]);

        // IFC cluster (absolute 8) maps the 32 FAT clusters at absolute 9..40.
        let cluster_len = PAGE_LEN * PAGES_PER_CLUSTER;
        let ifc_offset = 8 * cluster_len;
        for index in 0..32 {
            let fat_cluster = 9u32 + index as u32;
            let offset = ifc_offset + index * 4;
            data[offset..offset + 4].copy_from_slice(&fat_cluster.to_le_bytes());
        }
        data[ifc_offset + 32 * 4..ifc_offset + cluster_len].fill(0xFF);

        // The FAT marks the root directory's single cluster as allocated and
        // terminated. All other allocation entries remain free (zero).
        let first_fat_offset = 9 * cluster_len;
        data[first_fat_offset..first_fat_offset + 4]
            .copy_from_slice(&CLUSTER_LINK_END.to_le_bytes());

        // Root directory's first entry describes the root itself.
        let root_offset = ALLOC_OFFSET * cluster_len;
        data[root_offset..root_offset + 2]
            .copy_from_slice(&(DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY).to_le_bytes());
        data[root_offset + 4..root_offset + 8].copy_from_slice(&1u32.to_le_bytes());
        data[root_offset + 0x10..root_offset + 0x14].copy_from_slice(&0u32.to_le_bytes());
        data[root_offset + 0x40] = b'.';

        // Expand each logical page into the raw layout through the same helper
        // used by later writes, so its spare-area ECC is initialized too.
        let page_count = data_len / PAGE_LEN;
        let raw_page_len = PAGE_LEN + PAGE_SPARE_LEN;
        let mut bytes = vec![0; page_count * raw_page_len];
        for page_index in 0..page_count {
            let source = &data[page_index * PAGE_LEN..(page_index + 1) * PAGE_LEN];
            let target = page_index * raw_page_len;
            write_page_data(&mut bytes, target, PAGE_LEN, 0, source)
                .expect("new memory card page fits its raw image");
        }

        MemoryCard { bytes }
    }

    fn write_cluster_bytes(&mut self, cluster: u32, offset: usize, contents: &[u8]) -> Result<()> {
        let (page_len, pages_per_cluster, page_stride) = self.geometry()?;
        let cluster_len = page_len * pages_per_cluster;
        anyhow::ensure!(
            offset
                .checked_add(contents.len())
                .context("cluster write range overflow")?
                <= cluster_len,
            "cluster write exceeds cluster size"
        );
        let first_page = usize::try_from(cluster)?
            .checked_mul(pages_per_cluster)
            .context("cluster page offset overflow")?;
        let mut copied = 0;
        while copied < contents.len() {
            let logical_offset = offset + copied;
            let page = logical_offset / page_len;
            let in_page = logical_offset % page_len;
            let length = (page_len - in_page).min(contents.len() - copied);
            let physical = (first_page + page)
                .checked_mul(page_stride)
                .and_then(|value| value.checked_add(in_page))
                .context("page offset overflow")?;
            let end = physical
                .checked_add(length)
                .context("page range overflow")?;
            anyhow::ensure!(end <= self.bytes.len(), "writing memory card page");
            write_page_data(
                &mut self.bytes,
                physical - in_page,
                page_len,
                in_page,
                &contents[copied..copied + length],
            )?;
            copied += length;
        }
        Ok(())
    }

    fn set_fat_entry(&mut self, relative_cluster: u32, value: u32) -> Result<()> {
        let (page_len, pages_per_cluster, _) = self.geometry()?;
        let entries_per_cluster = page_len
            .checked_mul(pages_per_cluster)
            .context("cluster size overflow")?
            / 4;
        let fat_cluster_index = usize::try_from(relative_cluster)? / entries_per_cluster;
        let entry_index = usize::try_from(relative_cluster)? % entries_per_cluster;
        let indirect_index = fat_cluster_index / entries_per_cluster;
        let indirect_entry_index = fat_cluster_index % entries_per_cluster;
        let ifc_cluster = self.get_superblock_ifc_list_item(indirect_index)?;
        anyhow::ensure!(
            ifc_cluster != CLUSTER_LINK_END,
            "missing indirect FAT cluster"
        );
        let fat_cluster = get_u32(&self.read_cluster(ifc_cluster)?, indirect_entry_index * 4)?;
        anyhow::ensure!(fat_cluster != CLUSTER_LINK_END, "missing FAT cluster entry");
        // IFC entries contain physical cluster numbers, as used by `fat_entry`.
        self.write_cluster_bytes(fat_cluster, entry_index * 4, &value.to_le_bytes())
    }

    fn allocate_free_cluster(&mut self) -> Result<u32> {
        let alloc_start = self.get_superblock_alloc_offset()?;
        let alloc_count = self
            .get_superblock_clusters_per_card()?
            .checked_sub(alloc_start)
            .context("allocation start is beyond card")?;
        for cluster in 0..alloc_count {
            if self.fat_entry(cluster)? == 0 {
                self.set_fat_entry(cluster, CLUSTER_LINK_END)?;
                let cluster_len =
                    self.geometry()?.0 * usize::from(self.get_superblock_pages_per_cluster()?);
                self.write_cluster_bytes(alloc_start + cluster, 0, &vec![0; cluster_len])?;
                return Ok(cluster);
            }
        }
        anyhow::bail!("memory card has insufficient free space")
    }

    fn find_or_grow_directory_slot(
        &mut self,
        directory: u32,
        contents: &mut Vec<u8>,
        first_child: usize,
    ) -> Result<usize> {
        let capacity = contents.len() / DIR_ENTRY_LENGTH;
        if let Some(slot) = (first_child..capacity).find(|index| {
            get_u16(contents, index * DIR_ENTRY_LENGTH)
                .is_ok_and(|mode| mode & DIR_ENTRY_MODE_EXISTS == 0)
        }) {
            return Ok(slot);
        }

        let cluster_len =
            self.geometry()?.0 * usize::from(self.get_superblock_pages_per_cluster()?);
        anyhow::ensure!(
            cluster_len % DIR_ENTRY_LENGTH == 0,
            "directory cluster cannot hold whole entries"
        );
        let chain = self.cluster_chain(directory)?;
        let last = *chain
            .last()
            .context("directory has an empty cluster chain")?;
        let new_cluster = self.allocate_free_cluster()?;
        self.set_fat_entry(last, 0x8000_0000 | new_cluster)?;
        contents.resize(contents.len() + cluster_len, 0);
        Ok(capacity)
    }

    fn delete_directory_contents(
        &mut self,
        first_cluster: u32,
        entry_count: u32,
        depth: usize,
        clusters_to_free: &mut Vec<u32>,
    ) -> Result<()> {
        anyhow::ensure!(depth <= 64, "directory nesting exceeds 64 levels");
        let alloc_start = self.get_superblock_alloc_offset()?;
        let chain = self.cluster_chain(first_cluster)?;
        let mut contents = Vec::new();
        for cluster in &chain {
            contents.extend(
                self.read_cluster(
                    alloc_start
                        .checked_add(*cluster)
                        .context("physical cluster number overflow")?,
                )?,
            );
        }
        let count = usize::try_from(entry_count)?;
        anyhow::ensure!(
            count <= contents.len() / DIR_ENTRY_LENGTH,
            "directory entry count exceeds its cluster chain"
        );
        let cluster_len =
            self.geometry()?.0 * usize::from(self.get_superblock_pages_per_cluster()?);

        for index in 2..count {
            let offset = index * DIR_ENTRY_LENGTH;
            let mode = get_u16(&contents, offset)?;
            if mode & DIR_ENTRY_MODE_EXISTS == 0 {
                continue;
            }
            let child_cluster = get_u32(&contents, offset + 0x10)?;
            if mode & DIR_ENTRY_MODE_DIRECTORY != 0 {
                self.delete_directory_contents(
                    child_cluster,
                    get_u32(&contents, offset + 4)?,
                    depth + 1,
                    clusters_to_free,
                )?;
                clusters_to_free.extend(self.cluster_chain(child_cluster)?);
            } else if mode & DIR_ENTRY_MODE_FILE != 0 && get_u32(&contents, offset + 4)? > 0 {
                clusters_to_free.extend(self.cluster_chain(child_cluster)?);
            }
            let physical_dir_cluster = alloc_start
                .checked_add(chain[offset / cluster_len])
                .context("physical directory cluster overflow")?;
            self.write_cluster_bytes(physical_dir_cluster, offset % cluster_len, &[0, 0])?;
        }
        clusters_to_free.extend(chain);
        Ok(())
    }

    pub fn remove_path(&mut self, path: &UnixPath) -> Result<()> {
        let components: Vec<String> = path
            .components()
            .filter_map(|component| match component {
                UnixComponent::RootDir | UnixComponent::CurDir => None,
                UnixComponent::Normal(name) => Some(String::from_utf8_lossy(name).into_owned()),
                UnixComponent::ParentDir => Some(String::new()),
            })
            .collect();
        anyhow::ensure!(
            !components.is_empty(),
            "cannot remove the memory card root directory"
        );
        anyhow::ensure!(
            !components.iter().any(String::is_empty),
            "path must not contain parent or prefix components"
        );

        let mut directory = self.get_superblock_rootdir_cluster()?;
        for component in &components[..components.len() - 1] {
            Self::validate_entry_name(component)?;
            let contents = self.read_cluster_chain(directory)?;
            let count = usize::try_from(get_u32(&contents, 4)?)?;
            anyhow::ensure!(
                count <= contents.len() / DIR_ENTRY_LENGTH,
                "directory entry count exceeds its cluster chain"
            );
            let first_child = if directory == self.get_superblock_rootdir_cluster()? {
                1
            } else {
                2
            };
            let mut found = None;
            for index in first_child..count {
                let offset = index * DIR_ENTRY_LENGTH;
                let mode = get_u16(&contents, offset)?;
                if mode & (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
                    != (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
                {
                    continue;
                }
                let bytes = &contents[offset + 0x40..offset + 0x60];
                let end = bytes
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(bytes.len());
                if bytes[..end].eq_ignore_ascii_case(component.as_bytes()) {
                    anyhow::ensure!(found.is_none(), "multiple entries named {component:?}");
                    found = Some(get_u32(&contents, offset + 0x10)?);
                }
            }
            directory = found.with_context(|| format!("directory {component:?} not found"))?;
        }

        let name = components.last().expect("nonempty path");
        Self::validate_entry_name(name)?;
        let contents = self.read_cluster_chain(directory)?;
        let count = usize::try_from(get_u32(&contents, 4)?)?;
        anyhow::ensure!(
            count <= contents.len() / DIR_ENTRY_LENGTH,
            "directory entry count exceeds its cluster chain"
        );
        let first_child = if directory == self.get_superblock_rootdir_cluster()? {
            1
        } else {
            2
        };
        let mut found = None;
        for index in first_child..count {
            let offset = index * DIR_ENTRY_LENGTH;
            if get_u16(&contents, offset)? & DIR_ENTRY_MODE_EXISTS == 0 {
                continue;
            }
            let bytes = &contents[offset + 0x40..offset + 0x60];
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len());
            if bytes[..end].eq_ignore_ascii_case(name.as_bytes()) {
                anyhow::ensure!(found.is_none(), "multiple entries named {name:?}");
                found = Some((
                    index,
                    get_u16(&contents, offset)?,
                    get_u32(&contents, offset + 4)?,
                    get_u32(&contents, offset + 0x10)?,
                ));
            }
        }
        let (index, mode, size, first_cluster) = found.context("path not found")?;
        anyhow::ensure!(
            mode & (DIR_ENTRY_MODE_DIRECTORY | DIR_ENTRY_MODE_FILE) != 0,
            "path is not a file or directory"
        );

        let mut clusters_to_free = Vec::new();
        if mode & DIR_ENTRY_MODE_DIRECTORY != 0 {
            self.delete_directory_contents(first_cluster, size, 0, &mut clusters_to_free)?;
        } else if size > 0 {
            clusters_to_free.extend(self.cluster_chain(first_cluster)?);
        }
        clusters_to_free.sort_unstable();
        clusters_to_free.dedup();
        for cluster in clusters_to_free {
            self.set_fat_entry(cluster, 0)?;
        }

        let chain = self.cluster_chain(directory)?;
        let entry_offset = index * DIR_ENTRY_LENGTH;
        let cluster_len =
            self.geometry()?.0 * usize::from(self.get_superblock_pages_per_cluster()?);
        let alloc_start = self.get_superblock_alloc_offset()?;
        let physical = alloc_start
            .checked_add(chain[entry_offset / cluster_len])
            .context("physical directory cluster overflow")?;
        self.write_cluster_bytes(physical, entry_offset % cluster_len, &[0, 0])?;
        Ok(())
    }

    pub fn delete_save_folder(&mut self, name: &str) -> Result<()> {
        let root_cluster = self.get_superblock_rootdir_cluster()?;
        let alloc_start = self.get_superblock_alloc_offset()?;
        let root_chain = self.cluster_chain(root_cluster)?;
        let mut root = Vec::new();
        for cluster in &root_chain {
            root.extend(
                self.read_cluster(
                    alloc_start
                        .checked_add(*cluster)
                        .context("physical cluster number overflow")?,
                )?,
            );
        }
        let count = usize::try_from(get_u32(&root, 4)?)?;
        anyhow::ensure!(
            count <= root.len() / DIR_ENTRY_LENGTH,
            "root directory entry count exceeds its cluster chain"
        );
        let mut found = None;
        for index in 1..count {
            let offset = index * DIR_ENTRY_LENGTH;
            let mode = get_u16(&root, offset)?;
            if mode & (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
                != (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
            {
                continue;
            }
            let bytes = &root[offset + 0x40..offset + 0x60];
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len());
            if bytes[..end].eq_ignore_ascii_case(name.as_bytes()) {
                anyhow::ensure!(found.is_none(), "multiple save folders named {name:?}");
                found = Some((
                    index,
                    get_u32(&root, offset + 4)?,
                    get_u32(&root, offset + 0x10)?,
                ));
            }
        }
        let (index, entry_count, folder_cluster) = found.context("save folder not found")?;
        let mut to_free = Vec::new();
        self.delete_directory_contents(folder_cluster, entry_count, 0, &mut to_free)?;
        to_free.sort_unstable();
        to_free.dedup();
        for cluster in to_free {
            self.set_fat_entry(cluster, 0)?;
        }

        let mode_offset = index * DIR_ENTRY_LENGTH;
        let cluster_len =
            self.geometry()?.0 * usize::from(self.get_superblock_pages_per_cluster()?);
        let physical = alloc_start
            .checked_add(root_chain[mode_offset / cluster_len])
            .context("physical root cluster overflow")?;
        let local_offset = mode_offset % cluster_len;
        self.write_cluster_bytes(physical, local_offset, &[0, 0])?;
        Ok(())
    }

    pub fn copy_save_folder_from(&mut self, source: &MemoryCard, name: &str) -> Result<()> {
        let root = source.read_cluster_chain(source.get_superblock_rootdir_cluster()?)?;
        let count = usize::try_from(get_u32(&root, 4)?)?;
        let mut source_entry = None;
        for index in 1..count {
            let offset = index * DIR_ENTRY_LENGTH;
            let mode = get_u16(&root, offset)?;
            let bytes = &root[offset + 0x40..offset + 0x60];
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len());
            if mode & (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
                == (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
                && bytes[..end].eq_ignore_ascii_case(name.as_bytes())
            {
                anyhow::ensure!(
                    source_entry.is_none(),
                    "multiple save folders named {name:?}"
                );
                source_entry = Some(root[offset..offset + DIR_ENTRY_LENGTH].to_vec());
            }
        }
        let entry = source_entry.context("save folder not found")?;
        let dst_root_chain = self.cluster_chain(self.get_superblock_rootdir_cluster()?)?;
        let dst_root = self.read_cluster_chain(self.get_superblock_rootdir_cluster()?)?;
        let dst_count = usize::try_from(get_u32(&dst_root, 4)?)?;
        for index in 1..dst_count {
            let offset = index * DIR_ENTRY_LENGTH;
            let mode = get_u16(&dst_root, offset)?;
            let bytes = &dst_root[offset + 0x40..offset + 0x60];
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len());
            anyhow::ensure!(
                mode & DIR_ENTRY_MODE_EXISTS == 0
                    || !bytes[..end].eq_ignore_ascii_case(name.as_bytes()),
                "save folder {name:?} already exists on destination"
            );
        }
        let existing_slot = (1..dst_root.len() / DIR_ENTRY_LENGTH).find(|index| {
            get_u16(&dst_root, index * DIR_ENTRY_LENGTH)
                .is_ok_and(|mode| mode & DIR_ENTRY_MODE_EXISTS == 0)
        });
        let free_slot = existing_slot.unwrap_or(dst_root.len() / DIR_ENTRY_LENGTH);
        let grow_root = existing_slot.is_none();

        let mut dir_specs = Vec::new();
        let mut all_chains: Vec<Vec<u32>> = Vec::new();
        let mut pending = vec![(get_u32(&entry, 0x10)?, get_u32(&entry, 4)?, None)];
        let mut visited = Vec::new();
        while let Some((first, entries, parent)) = pending.pop() {
            anyhow::ensure!(
                !visited.contains(&first),
                "directory cycle detected at cluster {first}"
            );
            visited.push(first);
            let chain = source.cluster_chain(first)?;
            let contents = source.read_cluster_chain(first)?;
            anyhow::ensure!(
                usize::try_from(entries)? <= contents.len() / DIR_ENTRY_LENGTH,
                "directory entry count exceeds its cluster chain"
            );
            for i in 2..usize::try_from(entries)? {
                let off = i * DIR_ENTRY_LENGTH;
                let mode = get_u16(&contents, off)?;
                if mode & DIR_ENTRY_MODE_EXISTS == 0 {
                    continue;
                }
                let child = get_u32(&contents, off + 0x10)?;
                if mode & DIR_ENTRY_MODE_DIRECTORY != 0 {
                    pending.push((child, get_u32(&contents, off + 4)?, Some(first)));
                } else if mode & DIR_ENTRY_MODE_FILE != 0 && get_u32(&contents, off + 4)? > 0 {
                    all_chains.push(source.cluster_chain(child)?);
                }
            }
            all_chains.push(chain.clone());
            dir_specs.push((first, entries, chain, parent));
        }

        let mut allocated = Vec::new();
        for rel in
            0..self.get_superblock_clusters_per_card()? - self.get_superblock_alloc_offset()?
        {
            if self.fat_entry(rel)? == 0 {
                allocated.push(rel);
            }
        }
        let required: usize = all_chains.iter().map(Vec::len).sum();
        anyhow::ensure!(
            allocated.len() >= required + usize::from(grow_root),
            "destination memory card has insufficient free space"
        );
        let alloc_start = self.get_superblock_alloc_offset()?;
        let mut mappings = std::collections::HashMap::new();
        let mut next = allocated.into_iter();
        for chain in &all_chains {
            for &old in chain {
                mappings
                    .entry(old)
                    .or_insert_with(|| next.next().expect("capacity checked"));
            }
        }
        let grown_root_cluster = if grow_root {
            let cluster = next
                .next()
                .context("memory card has insufficient free space")?;
            let last = *dst_root_chain
                .last()
                .context("destination root has an empty cluster chain")?;
            self.set_fat_entry(last, 0x8000_0000 | cluster)?;
            self.set_fat_entry(cluster, CLUSTER_LINK_END)?;
            let cluster_len =
                self.geometry()?.0 * usize::from(self.get_superblock_pages_per_cluster()?);
            self.write_cluster_bytes(alloc_start + cluster, 0, &vec![0; cluster_len])?;
            Some(cluster)
        } else {
            None
        };
        for chain in &all_chains {
            let mapped: Vec<_> = chain.iter().map(|c| mappings[c]).collect();
            for (i, &cluster) in mapped.iter().enumerate() {
                let link = mapped
                    .get(i + 1)
                    .map(|c| 0x8000_0000 | c)
                    .unwrap_or(CLUSTER_LINK_END);
                self.set_fat_entry(cluster, link)?;
                let data = source.read_cluster(
                    source
                        .get_superblock_alloc_offset()?
                        .checked_add(chain[i])
                        .context("source cluster overflow")?,
                )?;
                self.write_cluster_bytes(
                    alloc_start
                        .checked_add(cluster)
                        .context("destination cluster overflow")?,
                    0,
                    &data,
                )?;
            }
        }
        for (first, entries, chain, parent) in &dir_specs {
            let mut contents = source.read_cluster_chain(*first)?;
            contents[0x10..0x14].copy_from_slice(&mappings[first].to_le_bytes());
            if let Some(parent) = parent {
                contents[DIR_ENTRY_LENGTH + 0x10..DIR_ENTRY_LENGTH + 0x14]
                    .copy_from_slice(&mappings[parent].to_le_bytes());
            } else {
                contents[DIR_ENTRY_LENGTH + 0x10..DIR_ENTRY_LENGTH + 0x14]
                    .copy_from_slice(&self.get_superblock_rootdir_cluster()?.to_le_bytes());
            }
            for i in 2..usize::try_from(*entries)? {
                let off = i * DIR_ENTRY_LENGTH;
                let mode = get_u16(&contents, off)?;
                if mode & DIR_ENTRY_MODE_EXISTS == 0 {
                    continue;
                }
                let old = get_u32(&contents, off + 0x10)?;
                if mappings.contains_key(&old) {
                    contents[off + 0x10..off + 0x14].copy_from_slice(&mappings[&old].to_le_bytes());
                }
            }
            for (ci, &old) in chain.iter().enumerate() {
                let len =
                    self.geometry()?.0 * usize::from(self.get_superblock_pages_per_cluster()?);
                self.write_cluster_bytes(
                    alloc_start + mappings[&old],
                    0,
                    &contents[ci * len..(ci + 1) * len],
                )?;
            }
        }
        let copied_first = mappings[&get_u32(&entry, 0x10)?];
        let mut copied_entry = entry;
        copied_entry[0x10..0x14].copy_from_slice(&copied_first.to_le_bytes());
        let cluster_len =
            self.geometry()?.0 * usize::from(self.get_superblock_pages_per_cluster()?);
        let offset = free_slot * DIR_ENTRY_LENGTH;
        if let Some(cluster) = grown_root_cluster {
            self.write_cluster_bytes(alloc_start + cluster, 0, &copied_entry)?;
        } else {
            let physical = alloc_start + dst_root_chain[offset / cluster_len];
            self.write_cluster_bytes(physical, offset % cluster_len, &copied_entry)?;
        }
        if free_slot >= dst_count {
            let count_offset = 4;
            let count_physical = alloc_start + dst_root_chain[count_offset / cluster_len];
            self.write_cluster_bytes(
                count_physical,
                count_offset % cluster_len,
                &u32::try_from(free_slot + 1)?.to_le_bytes(),
            )?;
        }
        Ok(())
    }

    fn read_dir<P: AsRef<UnixPath>>(&self, path: P) -> Result<fs::ReadDir> {
        let path = path.as_ref();

        if !path.is_absolute() {
            return Err(anyhow!("path is not absolute"));
        }

        let components: Vec<String> = path
            .components()
            .filter_map(|component| match component {
                UnixComponent::RootDir | UnixComponent::CurDir => None,
                UnixComponent::Normal(name) => Some(String::from_utf8_lossy(name).into_owned()),
                UnixComponent::ParentDir => Some(String::new()),
            })
            .collect();
        anyhow::ensure!(
            !components.iter().any(String::is_empty),
            "path must not contain parent or prefix components"
        );

        let root = self.get_superblock_rootdir_cluster()?;
        let mut directory = root;
        let root_contents = self.read_cluster_chain(root)?;
        let mut entry_count = usize::try_from(get_u32(&root_contents, 4)?)?;
        for component in &components {
            let contents = self.read_cluster_chain(directory)?;
            anyhow::ensure!(
                entry_count <= contents.len() / DIR_ENTRY_LENGTH,
                "directory entry count exceeds its cluster chain"
            );
            let first_child = if directory == root { 1 } else { 2 };
            let mut found = None;
            for index in first_child..entry_count {
                let offset = index * DIR_ENTRY_LENGTH;
                let mode = get_u16(&contents, offset)?;
                if mode & (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
                    != (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
                {
                    continue;
                }
                let bytes = &contents[offset + 0x40..offset + 0x60];
                let end = bytes
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(bytes.len());
                if bytes[..end].eq_ignore_ascii_case(component.as_bytes()) {
                    let child = get_u32(&contents, offset + 0x10)?;
                    // The entry's size field is not the live child count. The
                    // directory cluster header is updated as entries are added.
                    let child_contents = self.read_cluster_chain(child)?;
                    found = Some((child, usize::try_from(get_u32(&child_contents, 4)?)?));
                    break;
                }
            }
            (directory, entry_count) =
                found.with_context(|| format!("directory {component:?} not found"))?;
        }

        let contents = self.read_cluster_chain(directory)?;
        anyhow::ensure!(
            entry_count <= contents.len() / DIR_ENTRY_LENGTH,
            "directory entry count exceeds its cluster chain"
        );
        let first_child = if directory == root { 1 } else { 2 };
        let mut entries = Vec::new();
        let mut parent = UnixPath::new("/").to_path_buf();
        for component in &components {
            parent.push(component);
        }
        for index in first_child..entry_count {
            let offset = index * DIR_ENTRY_LENGTH;
            let mode = get_u16(&contents, offset)?;
            if mode & DIR_ENTRY_MODE_EXISTS == 0 {
                continue;
            }
            let name_bytes = &contents[offset + 0x40..offset + 0x60];
            let end = name_bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(name_bytes.len());
            let name = String::from_utf8_lossy(&name_bytes[..end]).into_owned();
            if name.is_empty() || name == "." || name == ".." {
                continue;
            }
            let mut entry_path = parent.clone();
            entry_path.push(&name);
            let file_type = if mode & DIR_ENTRY_MODE_DIRECTORY != 0 {
                FileType::Directory
            } else {
                FileType::File
            };
            entries.push(DirEntry::new(entry_path, file_type));
        }
        Ok(fs::ReadDir::new(entries))
    }

    fn create_dir<P: AsRef<UnixPath>>(&mut self, path: P) -> Result<()> {
        let path = path.as_ref();

        if !path.is_absolute() {
            return Err(anyhow!("path is not absolute"));
        }

        let components: Vec<String> = path
            .components()
            .filter_map(|component| match component {
                UnixComponent::RootDir | UnixComponent::CurDir => None,
                UnixComponent::Normal(name) => Some(String::from_utf8_lossy(name).into_owned()),
                UnixComponent::ParentDir => Some(String::new()),
            })
            .collect();
        anyhow::ensure!(
            !components.is_empty(),
            "cannot create the memory card root directory"
        );
        anyhow::ensure!(
            !components.iter().any(String::is_empty),
            "path must not contain parent or prefix components"
        );

        let name = components.last().expect("nonempty path");
        Self::validate_entry_name(name)?;
        anyhow::ensure!(
            name.len() <= 31,
            "memory card entry name is longer than 31 bytes: {name:?}"
        );

        let root = self.get_superblock_rootdir_cluster()?;
        let mut parent = root;
        for component in &components[..components.len() - 1] {
            Self::validate_entry_name(component)?;
            let contents = self.read_cluster_chain(parent)?;
            let count = usize::try_from(get_u32(&contents, 4)?)?;
            anyhow::ensure!(
                count <= contents.len() / DIR_ENTRY_LENGTH,
                "directory entry count exceeds its cluster chain"
            );
            let first_child = if parent == root { 1 } else { 2 };
            let mut found = None;
            for index in first_child..count {
                let offset = index * DIR_ENTRY_LENGTH;
                if get_u16(&contents, offset)? & (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
                    != (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
                {
                    continue;
                }
                let bytes = &contents[offset + 0x40..offset + 0x60];
                let end = bytes
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(bytes.len());
                if bytes[..end].eq_ignore_ascii_case(component.as_bytes()) {
                    found = Some(get_u32(&contents, offset + 0x10)?);
                    break;
                }
            }
            parent = found.with_context(|| format!("parent directory {component:?} not found"))?;
        }

        let mut contents = self.read_cluster_chain(parent)?;
        let count = usize::try_from(get_u32(&contents, 4)?)?;
        anyhow::ensure!(
            count <= contents.len() / DIR_ENTRY_LENGTH,
            "directory entry count exceeds its cluster chain"
        );
        let first_child = if parent == root { 1 } else { 2 };
        for index in first_child..count {
            let offset = index * DIR_ENTRY_LENGTH;
            if get_u16(&contents, offset)? & DIR_ENTRY_MODE_EXISTS == 0 {
                continue;
            }
            let bytes = &contents[offset + 0x40..offset + 0x60];
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len());
            anyhow::ensure!(
                !bytes[..end].eq_ignore_ascii_case(name.as_bytes()),
                "entry already exists: {name:?}"
            );
        }
        let slot = self.find_or_grow_directory_slot(parent, &mut contents, first_child)?;

        let alloc_start = self.get_superblock_alloc_offset()?;
        let alloc_count = self
            .get_superblock_clusters_per_card()?
            .checked_sub(alloc_start)
            .context("allocation start is beyond card")?;
        let mut free = None;
        for cluster in 0..alloc_count {
            if self.fat_entry(cluster)? == 0 {
                free = Some(cluster);
                break;
            }
        }
        let cluster = free.context("memory card has insufficient free space")?;
        self.set_fat_entry(cluster, CLUSTER_LINK_END)?;

        let mut child_contents = vec![0; self.geometry()?.0 * self.geometry()?.1];
        for (index, (entry_name, entry_cluster)) in
            [(".", cluster), ("..", parent)].into_iter().enumerate()
        {
            let mut entry = vec![0; DIR_ENTRY_LENGTH];
            entry[0..2]
                .copy_from_slice(&(DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY).to_le_bytes());
            entry[4..8].copy_from_slice(&2u32.to_le_bytes());
            entry[0x10..0x14].copy_from_slice(&entry_cluster.to_le_bytes());
            entry[0x40..0x40 + entry_name.len()].copy_from_slice(entry_name.as_bytes());
            child_contents[index * DIR_ENTRY_LENGTH..(index + 1) * DIR_ENTRY_LENGTH]
                .copy_from_slice(&entry);
        }
        self.write_cluster_bytes(alloc_start + cluster, 0, &child_contents)?;

        let offset = slot * DIR_ENTRY_LENGTH;
        contents[offset..offset + DIR_ENTRY_LENGTH].fill(0);
        contents[offset..offset + 2]
            .copy_from_slice(&(DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY).to_le_bytes());
        contents[offset + 4..offset + 8].copy_from_slice(&2u32.to_le_bytes());
        contents[offset + 0x10..offset + 0x14].copy_from_slice(&cluster.to_le_bytes());
        contents[offset + 0x40..offset + 0x40 + name.len()].copy_from_slice(name.as_bytes());
        contents[4..8].copy_from_slice(&u32::try_from(count.max(slot + 1))?.to_le_bytes());

        let chain = self.cluster_chain(parent)?;
        let cluster_len =
            self.geometry()?.0 * usize::from(self.get_superblock_pages_per_cluster()?);
        for (index, &relative_cluster) in chain.iter().enumerate() {
            let start = index * cluster_len;
            if start >= contents.len() {
                break;
            }
            let end = (start + cluster_len).min(contents.len());
            self.write_cluster_bytes(alloc_start + relative_cluster, 0, &contents[start..end])?;
        }
        Ok(())
    }

    fn write<P: AsRef<UnixPath>>(&mut self, path: P, contents: &[u8]) -> Result<()> {
        let path = path.as_ref();

        if !path.is_absolute() {
            return Err(anyhow!("path is not absolute"));
        }

        let file_size =
            u32::try_from(contents.len()).context("file is too large for a memory card")?;
        let components: Vec<String> = path
            .components()
            .filter_map(|component| match component {
                UnixComponent::RootDir | UnixComponent::CurDir => None,
                UnixComponent::Normal(name) => Some(String::from_utf8_lossy(name).into_owned()),
                UnixComponent::ParentDir => Some(String::new()),
            })
            .collect();
        anyhow::ensure!(
            !components.is_empty(),
            "cannot write a file at the memory card root"
        );
        anyhow::ensure!(
            !components.iter().any(String::is_empty),
            "path must not contain parent or prefix components"
        );
        let name = components.last().expect("nonempty path");
        Self::validate_entry_name(name)?;
        anyhow::ensure!(
            name.len() <= 31,
            "memory card entry name is longer than 31 bytes: {name:?}"
        );

        let root = self.get_superblock_rootdir_cluster()?;
        let mut parent = root;
        for component in &components[..components.len() - 1] {
            Self::validate_entry_name(component)?;
            let directory = self.read_cluster_chain(parent)?;
            let count = usize::try_from(get_u32(&directory, 4)?)?;
            anyhow::ensure!(
                count <= directory.len() / DIR_ENTRY_LENGTH,
                "directory entry count exceeds its cluster chain"
            );
            let first_child = if parent == root { 1 } else { 2 };
            let mut found = None;
            for index in first_child..count {
                let offset = index * DIR_ENTRY_LENGTH;
                let mode = get_u16(&directory, offset)?;
                if mode & (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
                    != (DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_DIRECTORY)
                {
                    continue;
                }
                let bytes = &directory[offset + 0x40..offset + 0x60];
                let end = bytes
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(bytes.len());
                if bytes[..end].eq_ignore_ascii_case(component.as_bytes()) {
                    found = Some(get_u32(&directory, offset + 0x10)?);
                    break;
                }
            }
            parent = found.with_context(|| format!("parent directory {component:?} not found"))?;
        }

        let mut directory = self.read_cluster_chain(parent)?;
        let count = usize::try_from(get_u32(&directory, 4)?)?;
        anyhow::ensure!(
            count <= directory.len() / DIR_ENTRY_LENGTH,
            "directory entry count exceeds its cluster chain"
        );
        let first_child = if parent == root { 1 } else { 2 };
        for index in first_child..count {
            let offset = index * DIR_ENTRY_LENGTH;
            if get_u16(&directory, offset)? & DIR_ENTRY_MODE_EXISTS == 0 {
                continue;
            }
            let bytes = &directory[offset + 0x40..offset + 0x60];
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len());
            anyhow::ensure!(
                !bytes[..end].eq_ignore_ascii_case(name.as_bytes()),
                "entry already exists: {name:?}"
            );
        }
        let slot = self.find_or_grow_directory_slot(parent, &mut directory, first_child)?;

        let cluster_len =
            self.geometry()?.0 * usize::from(self.get_superblock_pages_per_cluster()?);
        let needed = contents.len().div_ceil(cluster_len);
        let alloc_start = self.get_superblock_alloc_offset()?;
        let alloc_count = self
            .get_superblock_clusters_per_card()?
            .checked_sub(alloc_start)
            .context("allocation start is beyond card")?;
        let mut clusters = Vec::with_capacity(needed);
        if needed > 0 {
            for cluster in 0..alloc_count {
                if self.fat_entry(cluster)? == 0 {
                    clusters.push(cluster);
                    if clusters.len() == needed {
                        break;
                    }
                }
            }
        }
        anyhow::ensure!(
            clusters.len() == needed,
            "memory card has insufficient free space"
        );

        for (index, &cluster) in clusters.iter().enumerate() {
            let next = clusters
                .get(index + 1)
                .map(|next| 0x8000_0000 | next)
                .unwrap_or(CLUSTER_LINK_END);
            self.set_fat_entry(cluster, next)?;
            let start = index * cluster_len;
            let end = (start + cluster_len).min(contents.len());
            let mut block = vec![0; cluster_len];
            block[..end - start].copy_from_slice(&contents[start..end]);
            self.write_cluster_bytes(alloc_start + cluster, 0, &block)?;
        }
        let first_cluster = clusters.first().copied().unwrap_or(0);

        let offset = slot * DIR_ENTRY_LENGTH;
        directory[offset..offset + DIR_ENTRY_LENGTH].fill(0);
        directory[offset..offset + 2]
            .copy_from_slice(&(DIR_ENTRY_MODE_EXISTS | DIR_ENTRY_MODE_FILE).to_le_bytes());
        directory[offset + 4..offset + 8].copy_from_slice(&file_size.to_le_bytes());
        directory[offset + 0x10..offset + 0x14].copy_from_slice(&first_cluster.to_le_bytes());
        directory[offset + 0x40..offset + 0x40 + name.len()].copy_from_slice(name.as_bytes());
        directory[4..8].copy_from_slice(&u32::try_from(count.max(slot + 1))?.to_le_bytes());

        let parent_chain = self.cluster_chain(parent)?;
        for (index, &cluster) in parent_chain.iter().enumerate() {
            let start = index * cluster_len;
            if start >= directory.len() {
                break;
            }
            let end = (start + cluster_len).min(directory.len());
            self.write_cluster_bytes(alloc_start + cluster, 0, &directory[start..end])?;
        }
        Ok(())
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
        anyhow::ensure!(page_len == PAGE_LEN, "unsupported page length: {page_len}");
        anyhow::ensure!(
            pages_per_cluster == 1 || pages_per_cluster == 2,
            "unsupported pages-per-cluster value: {pages_per_cluster}"
        );
        anyhow::ensure!(clusters_total > 0, "memory card declares no clusters");

        let raw_page_len = page_len
            .checked_add(PAGE_SPARE_LEN)
            .context("raw page size overflow")?;
        let expected_raw_len = clusters_total
            .checked_mul(pages_per_cluster)
            .and_then(|pages| pages.checked_mul(raw_page_len))
            .context("raw memory card size overflow")?;
        anyhow::ensure!(
            self.bytes.len() >= expected_raw_len,
            "memory card image is shorter than its declared raw geometry"
        );
        Ok((page_len, pages_per_cluster, raw_page_len))
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

    pub fn get_all_bytes(&self) -> &Vec<u8> {
        &self.bytes
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

impl FileSystem<UnixPath> for MemoryCard {
    type DirEntry = fs::DirEntry;
    type ReadDir = fs::ReadDir;

    fn create_dir<PathRef: AsRef<UnixPath>>(&mut self, path: PathRef) -> io::Result<()> {
        self.create_dir(path).map_err(io::Error::other)
    }

    fn read<PathRef: AsRef<UnixPath>>(&self, path: PathRef) -> io::Result<Vec<u8>> {
        todo!()
    }

    fn read_dir<PathRef: AsRef<UnixPath>>(&self, path: PathRef) -> io::Result<Self::ReadDir> {
        self.read_dir(path).map_err(io::Error::other)
    }

    fn write<PathRef: AsRef<UnixPath>>(
        &mut self,
        path: PathRef,
        contents: &[u8],
    ) -> io::Result<()> {
        self.write(path, contents).map_err(io::Error::other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok() {
        let mut memory_card = MemoryCard::new();

        memory_card.create_dir("/foo").unwrap();
        memory_card.create_dir("/bar").unwrap();
        memory_card.create_dir("/baz").unwrap();

        memory_card.create_dir("/foo/bar").unwrap();
        memory_card.create_dir("/foo/bar/baz").unwrap();

        let mut entries_root: Vec<_> = memory_card
            .read_dir("/")
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect();

        entries_root.sort_by_key(|entry| entry.file_name());

        assert_eq!(entries_root.len(), 3);
        assert_eq!(entries_root[0].file_name(), "bar");
        assert_eq!(entries_root[0].file_type(), fs::FileType::Directory);
        assert_eq!(entries_root[1].file_name(), "baz");
        assert_eq!(entries_root[1].file_type(), fs::FileType::Directory);
        assert_eq!(entries_root[2].file_name(), "foo");
        assert_eq!(entries_root[2].file_type(), fs::FileType::Directory);

        let mut entries_foo: Vec<_> = memory_card
            .read_dir("/foo")
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect();

        entries_foo.sort_by_key(|entry| entry.file_name());

        assert_eq!(entries_foo.len(), 1);
        assert_eq!(entries_foo[0].file_name(), "bar");
        assert_eq!(entries_foo[0].file_type(), fs::FileType::Directory);

        let mut entries_foo_bar: Vec<_> = memory_card
            .read_dir("/foo/bar")
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect();

        entries_foo_bar.sort_by_key(|entry| entry.file_name());

        assert_eq!(entries_foo_bar.len(), 1);
        assert_eq!(entries_foo_bar[0].file_name(), "baz");
        assert_eq!(entries_foo_bar[0].file_type(), fs::FileType::Directory);
    }
}
