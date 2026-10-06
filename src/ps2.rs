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
