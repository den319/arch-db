use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
};

use crc32fast::hash;
use snap::raw::{Decoder, Encoder};

use crate::{bloom_filter::BloomFilter, engine::Value, error::Result, helper::is_sorted, sstable_manager::SSTable};

#[derive(Debug, Clone)]
pub struct BlockMeta {
    pub start_key: String,
    pub offset: u64,
    pub record_offset: BTreeMap<String, u64>,
}
#[derive(Debug, Clone)]
pub struct SSTableIndex {
    pub offsets: BTreeMap<String, u64>,
    pub blocks: Vec<BlockMeta>,
}

#[derive(Debug, Clone)]
pub struct FooterMetadata {
    pub index_offset: u64,
    pub index_size: u64,

    pub bloom_offset: u64,
    pub bloom_size: u64,
}

#[derive(Debug, Clone)]
pub struct BlockRecord {
    pub key: String,
    pub value: Value,
}

pub struct SSTableIterator {
    pub file: File,
    pub index: SSTableIndex,
    pub current_block: Option<usize>,
    pub block_records: Vec<BlockRecord>,
    pub current_record: usize,
    
}

pub struct SSTableWriter {
    file: File,
    offsets: BTreeMap<String, u64>,
    file_offset: u64,
    current_block_offsets: BTreeMap<String, u64>,
    block_size: usize,
    single_block: Vec<u8>,
    blocks: Vec<BlockMeta>,
    is_new_block: bool,
    bloom: BloomFilter,
}

pub const BLOCK_SIZE: usize = 40;
const MAGIC: &[u8; 4] = b"ARCH";
const FORMAT_VERSION: u32 = 1;
const HEADER_SIZE: u64 = 8;
const TARGET_RECORDS_PER_TABLE: usize = 3;

impl SSTable {
    pub fn overlaps(&self, min_key: &str, max_key: &str) -> bool {
        !(self.max_key.as_str() < min_key || self.min_key.as_str() > max_key)
    }

    pub fn contains_key_range(&self, key: &str) -> bool {
        key >= self.min_key.as_str() && key <= self.max_key.as_str()
    }
}

impl SSTableWriter {
    pub fn new(
        path: &str,
        expected_records: usize,
    ) -> Result<Self> {

        let mut file = File::create(path)?;

        file.write_all(MAGIC)?;
        file.write_all(&FORMAT_VERSION.to_be_bytes())?;

        Ok(Self {
            file,

            offsets: BTreeMap::new(),
            file_offset: HEADER_SIZE,

            current_block_offsets: BTreeMap::new(),

            block_size: 0,
            single_block: Vec::new(),

            blocks: Vec::new(),

            is_new_block: true,

            bloom: BloomFilter::with_rate(
                0.01,
                expected_records.max(1) as u32,
            ),
        })
    }

    pub fn append(
        &mut self,
        key: String,
        value: Value,
    ) -> Result<()> {

        const BLOCK_HEADER_SIZE: u64 = 12;

        let record = serialize_record(&key, &value);

        if self.block_size + record.len() > BLOCK_SIZE {

            let written =
                write_block(
                    &mut self.file,
                    &self.single_block,
                )?;

            self.file_offset += written;

            if let Some(last_block) = self.blocks.last_mut() {

                last_block.record_offset =
                    std::mem::take(
                        &mut self.current_block_offsets
                    );
            }

            self.single_block.clear();
            self.block_size = 0;

            self.is_new_block = true;
        }

        if self.is_new_block {

            self.blocks.push(BlockMeta {
                start_key: key.clone(),
                offset: self.file_offset,
                record_offset: BTreeMap::new(),
            });

            self.is_new_block = false;
        }

        let record_offset =
            self.file_offset
            + BLOCK_HEADER_SIZE
            + self.single_block.len() as u64;

        self.current_block_offsets
            .insert(key.clone(), record_offset);

        self.offsets
            .insert(key.clone(), record_offset);

        self.bloom.insert(&key);

        self.single_block.extend(&record);

        self.block_size += record.len();

        Ok(())
    }

    pub fn finish(&mut self)-> Result<SSTableIndex> {
        if !self.single_block.is_empty() {
            if let Some(last_block) = self.blocks.last_mut() {
                last_block.record_offset =
                    std::mem::take(
                        &mut self.current_block_offsets,
                    );
            }

            write_block(
                &mut self.file,
                &self.single_block,
            )?;
        }

        self.blocks.sort_by(|a, b| {
            a.start_key.cmp(&b.start_key)
        });

        let index = SSTableIndex {
            offsets: self.offsets.clone(),
            blocks: self.blocks.clone(),
        };

        let serialized_index =
            serialize_index(&index);

        let index_offset =
            self.file.seek(
                SeekFrom::Current(0)
            )?;

        self.file.write_all(&serialized_index)?;

        let serialized_bloom =
            serialize_bloom(&self.bloom);

        let bloom_offset =
            self.file.seek(
                SeekFrom::Current(0)
            )?;

        self.file.write_all(&serialized_bloom)?;

        let footer = FooterMetadata {
            index_offset,
            index_size: serialized_index.len() as u64,

            bloom_offset,
            bloom_size: serialized_bloom.len() as u64,
        };

        let footer_bytes =
            footer.serialize();

        self.file.write_all(&footer_bytes)?;

        self.file.write_all(
            &(footer_bytes.len() as u64)
                .to_le_bytes(),
        )?;

        Ok(index)
    }
}

impl SSTableIterator {
    pub fn new(path: &str, index: SSTableIndex) -> Result<Self> {
        let mut file = File::open(path)?;


        Ok(Self {
            file,
            index,
            current_block: None,
            block_records: Vec::new(),
            current_record: 0,
        })
    }

    fn load_current_block(&mut self) -> Result<bool> {

        let block_index= match self.current_block {
            Some(index) => index,
            None => return Ok(false),
        };

        if block_index >= self.index.blocks.len() {
            return Ok(false);
        }

        let offset = self.index.blocks[block_index].offset;

        self.block_records = read_block_from_file(&mut self.file, offset)?;

        self.current_record = 0;

        Ok(true)
    }

    pub fn load_next_block(&mut self) -> Result<bool> {
        let next = match self.current_block {
            None => 0,
            Some(i) => i + 1,
        };

        if next >= self.index.blocks.len() {
            return Ok(false);
        }

        self.current_block = Some(next);

        self.load_current_block()
    }

    pub fn next(
        &mut self,
    ) -> Result<Option<BlockRecord>> {

        loop {

            if self.current_record < self.block_records.len() {

                let record =
                    self.block_records[self.current_record]
                        .clone();

                self.current_record += 1;

                return Ok(Some(record));
            }

            if !self.load_next_block()? {
                return Ok(None);
            }
        }
    }

    pub fn seek(
        &mut self,
        key: &str,
    ) -> Result<()> {

        match find_block_index(&self.index, key) {

            Some(block_index) => {

                self.current_block = Some(block_index);

                self.load_current_block()?;

                self.current_record = self
                    .block_records
                    .iter()
                    .position(|record| record.key.as_str() >= key)
                    .unwrap_or(self.block_records.len());
            }

            None => {

                self.current_block = None;

                self.block_records.clear();

                self.current_record = 0;
            }
        }

        Ok(())
    }
}

impl FooterMetadata {
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.extend(self.index_offset.to_le_bytes());
        bytes.extend(self.index_size.to_le_bytes());

        bytes.extend(self.bloom_offset.to_le_bytes());
        bytes.extend(self.bloom_size.to_le_bytes());

        bytes
    }

    pub fn deserialize(bytes: &[u8]) -> Self {
        let index_offset = u64::from_le_bytes(bytes[0..8].try_into().unwrap());

        let index_size = u64::from_le_bytes(bytes[8..16].try_into().unwrap());

        let bloom_offset = u64::from_le_bytes(bytes[16..24].try_into().unwrap());

        let bloom_size = u64::from_le_bytes(bytes[24..32].try_into().unwrap());

        Self {
            index_offset,
            index_size,
            bloom_offset,
            bloom_size,
        }
    }
}

impl SSTableIndex {
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        let count = self.offsets.len() as u64;
        bytes.extend(count.to_le_bytes());

        for (key, offset) in &self.offsets {
            let key_bytes = key.as_bytes();

            let key_len = key_bytes.len() as u64;

            bytes.extend(key_len.to_le_bytes());
            bytes.extend(key_bytes);

            bytes.extend(offset.to_le_bytes());
        }

        bytes
    }

    pub fn deserialize(bytes: &[u8]) -> Self {
        use std::collections::BTreeMap;

        let mut pos = 0;

        let count = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());

        pos += 8;

        let mut offsets = BTreeMap::new();

        for _ in 0..count {
            let key_len = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap()) as usize;

            pos += 8;

            let key = String::from_utf8(bytes[pos..pos + key_len].to_vec()).unwrap();

            pos += key_len;

            let offset = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());

            pos += 8;

            offsets.insert(key, offset);
        }

        Self {
            offsets,
            blocks: vec![],
        }
    }
}

pub fn serialize_bloom(bloom: &BloomFilter) -> Vec<u8> {
    // Manual serialization to avoid bincode compatibility issues with custom serde impl
    let bits_bytes = bincode::serialize(bloom).expect("failed to serialize bloom");
    bits_bytes
}

pub fn deserialize_bloom(bytes: &[u8]) -> BloomFilter {
    bincode::deserialize(bytes).expect("failed to deserialize bloom")
}

fn serialize_record(key: &str, value: &Value) -> Vec<u8> {
    // Payload (everything except the checksum)
    let mut payload = Vec::new();

    payload.push(match value {
        Value::Data(_) => 1u8,
        Value::Tombstone => 0u8,
    });

    payload.extend(&(key.len() as u32).to_be_bytes());

    let value_bytes = match value {
        Value::Data(v) => v.as_bytes(),
        Value::Tombstone => b"",
    };

    payload.extend(&(value_bytes.len() as u32).to_be_bytes());

    payload.extend(key.as_bytes());
    payload.extend(value_bytes);

    // Compute checksum over the payload only
    let checksum = hash(&payload);

    // Final record = CRC32 + payload
    let mut record = Vec::new();

    record.extend(checksum.to_be_bytes());
    record.extend(payload);

    record
}

fn verify_header(file: &mut File) -> Result<()> {
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;

    if &magic != MAGIC {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid SSTable magic").into());
    }

    let mut version = [0u8; 4];
    file.read_exact(&mut version)?;

    let version = u32::from_be_bytes(version);

    if version != FORMAT_VERSION {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Unsupported SSTable version {}", version),
        ).into());
    }

    Ok(())
}

fn write_block(
    file: &mut File,
    block: &[u8],
) -> Result<u64> {
    const BLOCK_HEADER_SIZE: u64 = 12;

    let compressed = Encoder::new()
        .compress_vec(block)
        .expect("compression failed");

    let compressed_len = compressed.len() as u32;
    let original_len = block.len() as u32;

    let checksum = hash(&compressed);

    file.write_all(&compressed_len.to_be_bytes())?;
    file.write_all(&original_len.to_be_bytes())?;
    file.write_all(&checksum.to_be_bytes())?;
    file.write_all(&compressed)?;

    Ok(BLOCK_HEADER_SIZE + compressed.len() as u64)
}

pub fn write_sstable(
    path: &str,
    data: &[(String, Value)],
) -> Result<SSTableIndex> {

    // debug_assert!(is_sorted(data));

    assert!(is_sorted(data), "write_sstable() requires sorted keys");

    let mut writer =
        SSTableWriter::new(path, data.len())?;

    for (key, value) in data {
        writer.append(key.clone(), value.clone())?;
    }

    writer.finish()
}

pub fn read_sstable(path: &str) -> Result<Vec<(String, Value)>> {
    let mut file = File::open(path)?;

    verify_header(&mut file)?;

    // Read the footer to find where data blocks end (at index_offset)
    let footer = read_footer(path)?;
    let data_end = footer.index_offset;

    let mut bytes = Vec::new();

    loop {
        // Stop if we've reached the index section
        let pos = file.seek(SeekFrom::Current(0))?;
        if pos >= data_end {
            break;
        }

        // ---------- Read header ----------

        // compressed size
        let mut len_buf = [0u8; 4];
        file.read_exact(&mut len_buf)?;
        let compressed_len = u32::from_be_bytes(len_buf) as usize;

        // original size
        file.read_exact(&mut len_buf)?;
        let original_len = u32::from_be_bytes(len_buf) as usize;

        // checksum
        let mut checksum_buf = [0u8; 4];
        file.read_exact(&mut checksum_buf)?;
        let stored_checksum = u32::from_be_bytes(checksum_buf);

        // ---------- Read compressed block ----------

        let mut compressed = vec![0u8; compressed_len];
        file.read_exact(&mut compressed)?;

        let computed_checksum = hash(&compressed);

        if computed_checksum != stored_checksum {
            println!("CORRUPTED SSTABLE BLOCK DETECTED");
            return Ok(vec![]);
        }

        // ---------- Decompress ----------

        let block = Decoder::new()
            .decompress_vec(&compressed)
            .expect("Failed to decompress block");

        assert_eq!(block.len(), original_len);

        // Append decompressed bytes
        bytes.extend(block);
    }

    let mut result = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if i + 4 > bytes.len() {
            break;
        }

        let stored_crc = u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);

        i += 4;

        let payload_start = i;

        if i >= bytes.len() {
            break;
        }

        let record_type = bytes[i];
        i += 1;

        if i + 4 > bytes.len() {
            break;
        }

        let key_len =
            u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;

        i += 4;

        if i + 4 > bytes.len() {
            break;
        }

        let val_len =
            u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;

        i += 4;

        if i + key_len > bytes.len() {
            break;
        }

        let key = String::from_utf8(bytes[i..i + key_len].to_vec()).unwrap();

        i += key_len;

        let value = match record_type {
            1 => {
                if i + val_len > bytes.len() {
                    break;
                }

                let value = String::from_utf8(bytes[i..i + val_len].to_vec()).unwrap();

                i += val_len;

                Value::Data(value)
            }

            0 => {
                if i + val_len > bytes.len() {
                    break;
                }

                i += val_len;

                Value::Tombstone
            }

            _ => break,
        };

        let payload = &bytes[payload_start..i];

        let computed_crc = hash(payload);

        if computed_crc != stored_crc {
            println!("Corrupted record skipped: {}", key);

            continue;
        }

        result.push((key, value));
    }

    Ok(result)
}

pub fn search_sstable(
    path: &str,
    index: &SSTableIndex,
    key: &str,
) -> Result<Option<(String, Value)>> {
    let block = match find_block(index, key) {
        Some(o) => o,
        None => return Ok(None),
    };

    // Read and decompress the entire block, then search for the key
    let records = read_block(path, block.offset)?;

    for record in &records {
        if record.key == key {
            return Ok(Some((record.key.clone(), record.value.clone())));
        }
    }

    Ok(None)
}

pub fn find_block_index(
    index: &SSTableIndex,
    key: &str,
) -> Option<usize> {

    let mut candidate = None;

    for (i, block) in index.blocks.iter().enumerate() {
        if key >= block.start_key.as_str() {
            candidate = Some(i);
        } else {
            break;
        }
    }

    candidate
}

pub fn find_block<'a>(
    index: &'a SSTableIndex,
    key: &str,
) -> Option<&'a BlockMeta> {

    // println!("Looking for {}", key);

    // for block in &index.blocks {
    //     println!(
    //         "start={} offset={}",
    //         block.start_key,
    //         block.offset
    //     );
    // }

    let block_index = find_block_index(index, key)?;

    index.blocks.get(block_index)
}


pub fn read_block(path: &str, offset: u64) -> Result<Vec<BlockRecord>> {
    let mut file = File::open(path)?;

    verify_header(&mut file)?;
    read_block_from_file(&mut file, offset)
}

pub fn read_block_from_file(file: &mut File, offset: u64) -> Result<Vec<BlockRecord>> {
    // Each block starts with: 4-byte data_len + 4-byte CRC32 + data
    // offset points to the start of the header
    file.seek(SeekFrom::Start(offset))?;

    // compressed block size
    let mut len_buf = [0u8; 4];
    file.read_exact(&mut len_buf)?;
    let compressed_len = u32::from_be_bytes(len_buf) as usize;

    // original block size
    file.read_exact(&mut len_buf)?;
    let original_len = u32::from_be_bytes(len_buf) as usize;

    // checksum
    let mut checksum_buf = [0u8; 4];
    file.read_exact(&mut checksum_buf)?;
    let stored_checksum = u32::from_be_bytes(checksum_buf);

    let mut compressed = vec![0u8; compressed_len];
    file.read_exact(&mut compressed)?;

    let computed_checksum = hash(&compressed);

    if computed_checksum != stored_checksum {
        println!("CORRUPTED SSTABLE BLOCK DETECTED");
        return Ok(vec![]);
    }

    // ---------- Decompress ----------

    let block = Decoder::new()
        .decompress_vec(&compressed)
        .expect("Failed to decompress block");

    if block.len() != original_len {
        println!("Corrupted block: decompressed size mismatch");
        return Ok(vec![]);
    }

    let mut result: Vec<BlockRecord> = vec![];

    let mut i = 0;
    while i < block.len() {
        // Each record starts with 4-byte CRC (written by serialize_record)
        if i + 4 > block.len() {
            break;
        }

        let stored_crc = u32::from_be_bytes([block[i], block[i + 1], block[i + 2], block[i + 3]]);

        i += 4;

        if i >= block.len() {
            break;
        }

        let payload_start = i;

        let record_type = block[i];
        i += 1;

        if i + 8 > block.len() {
            break;
        }

        let key_len =
            u32::from_be_bytes([block[i], block[i + 1], block[i + 2], block[i + 3]]) as usize;

        i += 4;

        let val_len =
            u32::from_be_bytes([block[i], block[i + 1], block[i + 2], block[i + 3]]) as usize;

        i += 4;

        if i + key_len > block.len() {
            break;
        }

        let key = String::from_utf8(block[i..i + key_len].to_vec()).unwrap();

        i += key_len;

        let value = match record_type {
            1 => {
                if i + val_len > block.len() {
                    break;
                }

                let value = String::from_utf8(block[i..i + val_len].to_vec()).unwrap();

                i += val_len;

                Value::Data(value)
            }

            0 => {
                if i + val_len > block.len() {
                    break;
                }

                i += val_len;

                Value::Tombstone
            }

            _ => break,
        };

        // Verify per-record CRC
        let payload = &block[payload_start..i];
        let computed_crc = hash(payload);

        if computed_crc != stored_crc {
            println!("Corrupted record skipped: {}", key);
            continue;
        }

        result.push(BlockRecord { key, value });
    }

    Ok(result)
}

pub fn serialize_index(index: &SSTableIndex) -> Vec<u8> {
    let mut bytes = Vec::new();

    bytes.extend(&(index.blocks.len() as u32).to_be_bytes());

    for block in &index.blocks {
        bytes.extend(&(block.start_key.len() as u32).to_be_bytes());
        bytes.extend(block.start_key.as_bytes());
        bytes.extend(block.offset.to_be_bytes());

        // Serialize record_offset entries for this block
        bytes.extend(&(block.record_offset.len() as u32).to_be_bytes());
        for (key, off) in &block.record_offset {
            bytes.extend(&(key.len() as u32).to_be_bytes());
            bytes.extend(key.as_bytes());
            bytes.extend(off.to_be_bytes());
        }
    }

    bytes
}

pub fn deserialize_index(bytes: &[u8]) -> SSTableIndex {
    let mut i = 0;

    let block_count =
        u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
    i += 4;
    let mut blocks = vec![];
    for _ in 0..block_count {
        let key_len =
            u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
        i += 4;
        let start_key = String::from_utf8(bytes[i..i + key_len].to_vec()).unwrap();
        i += key_len;
        let offset = u64::from_be_bytes([
            bytes[i],
            bytes[i + 1],
            bytes[i + 2],
            bytes[i + 3],
            bytes[i + 4],
            bytes[i + 5],
            bytes[i + 6],
            bytes[i + 7],
        ]);
        i += 8;

        // Deserialize record_offset entries for this block
        let record_count =
            u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
        i += 4;
        let mut record_offset = BTreeMap::new();
        for _ in 0..record_count {
            let rkey_len =
                u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
            i += 4;
            let rkey = String::from_utf8(bytes[i..i + rkey_len].to_vec()).unwrap();
            i += rkey_len;
            let roff = u64::from_be_bytes([
                bytes[i],
                bytes[i + 1],
                bytes[i + 2],
                bytes[i + 3],
                bytes[i + 4],
                bytes[i + 5],
                bytes[i + 6],
                bytes[i + 7],
            ]);
            i += 8;
            record_offset.insert(rkey, roff);
        }

        blocks.push(BlockMeta {
            start_key,
            offset,
            record_offset,
        });
    }
    SSTableIndex {
        offsets: BTreeMap::new(),
        blocks,
    }
}

pub fn read_footer(path: &str) -> Result<FooterMetadata> {
    let mut file = File::open(path)?;

    verify_header(&mut file)?;

    let file_size = file.metadata()?.len();

    if file_size < 8 {
        return Err(format!("File too small to contain footer: {} bytes", file_size).into());
    }

    file.seek(SeekFrom::Start(file_size - 8))?;

    let mut size_buf = [0u8; 8];

    file.read_exact(&mut size_buf)?;

    let footer_size = u64::from_le_bytes(size_buf);

    if file_size < 8 + footer_size {
        return Err(format!(
            "Corrupted SSTable: footer_size {} exceeds file_size {}",
            footer_size, file_size
        )
        .into());
    }

    file.seek(SeekFrom::Start(file_size - 8 - footer_size))?;

    let mut footer_bytes = vec![0u8; footer_size as usize];

    file.read_exact(&mut footer_bytes)?;

    Ok(FooterMetadata::deserialize(&footer_bytes))
}

pub fn load_index_from_footer(path: &str) -> Result<SSTableIndex> {
    let footer = read_footer(path)?;

    let mut file = File::open(path)?;


    file.seek(SeekFrom::Start(footer.index_offset))?;

    let mut bytes = vec![0u8; footer.index_size as usize];

    file.read_exact(&mut bytes)?;

    Ok(deserialize_index(&bytes))
}

pub fn load_bloom_from_footer(path: &str) -> Result<BloomFilter> {
    let footer = read_footer(path)?;

    let mut file = File::open(path)?;


    file.seek(SeekFrom::Start(footer.bloom_offset))?;

    let mut bytes = vec![0u8; footer.bloom_size as usize];

    file.read_exact(&mut bytes)?;

    Ok(deserialize_bloom(&bytes))
}

pub fn binary_search_block(records: &[BlockRecord], key: &str) -> Option<Value> {
    match records.binary_search_by(|record| record.key.as_str().cmp(key)) {
        Ok(index) => Some(records[index].value.clone()),
        Err(_) => None,
    }
}
