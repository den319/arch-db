use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
};

use crc32fast::hash;

use crate::{engine::Value, error::Result, sstable_manager::SSTable};

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
pub struct SSTableFooter {
    pub index_offset: u64,
    pub bloom_offset: u64,
}
pub const BLOCK_SIZE: usize = 40;

impl SSTable {
    pub fn overlaps(&self, min_key: &str, max_key: &str) -> bool {
        !(self.max_key.as_str() < min_key || self.min_key.as_str() > max_key)
    }

    pub fn contains_key_range(&self, key: &str) -> bool {
        key >= self.min_key.as_str() && key <= self.max_key.as_str()
    }
}

pub fn write_sstable(path: &str, data: &[(String, Value)]) -> Result<SSTableIndex> {
    const HEADER_SIZE: u64 = 8; // 4 bytes data_len + 4 bytes CRC32

    let mut offsets = BTreeMap::new();
    let mut file_offset = 0u64;

    let mut current_block_offsets = BTreeMap::new();

    let mut block_size = 0usize;

    let mut single_block = Vec::new();

    let mut blocks: Vec<BlockMeta> = Vec::new();

    let mut file = File::create(path)?;

    let mut is_new_block = true;

    // println!("data: {:?}", data);

    for (key, val) in data {
        let mut record = Vec::new();

        record.push(match val {
            Value::Data(_) => 1u8,
            Value::Tombstone => 0u8,
        });

        record.extend(&(key.len() as u32).to_be_bytes());

        let value_bytes = match val {
            Value::Data(v) => v.as_bytes(),
            Value::Tombstone => b"",
        };

        record.extend(&(value_bytes.len() as u32).to_be_bytes());

        record.extend(key.as_bytes());
        record.extend(value_bytes);

        if block_size + record.len() > BLOCK_SIZE {
            let checksum = hash(&single_block);
            let data_len = single_block.len() as u32;

            file.write_all(&data_len.to_be_bytes())?;
            file.write_all(&checksum.to_be_bytes())?;
            file.write_all(&single_block)?;

            file_offset += HEADER_SIZE + single_block.len() as u64;

            if let Some(last_block) = blocks.last_mut() {
                last_block.record_offset = current_block_offsets.clone();
            }

            current_block_offsets.clear();
            single_block.clear();
            block_size = 0;

            is_new_block = true;
        }

        if is_new_block {
            blocks.push(BlockMeta {
                start_key: key.clone(),
                offset: file_offset,
                record_offset: BTreeMap::new(),
            });
            is_new_block = false;
        }

        let record_offset = HEADER_SIZE + file_offset + single_block.len() as u64;

        current_block_offsets.insert(key.clone(), record_offset);

        single_block.extend(&record);
        block_size += record.len();

        offsets.insert(key.clone(), record_offset);
    }

    if !single_block.is_empty() {
        if let Some(last_block) = blocks.last_mut() {
            last_block.record_offset = current_block_offsets.clone();
        }

        let checksum = hash(&single_block);
        let data_len = single_block.len() as u32;

        file.write_all(&data_len.to_be_bytes())?;
        file.write_all(&checksum.to_be_bytes())?;
        file.write_all(&single_block)?;
    }

    blocks.sort_by(|a, b| a.start_key.cmp(&b.start_key));

    let index_offset= file.seek(SeekFrom::Current(0))?;

    let temp_index= SSTableIndex {
        offsets: offsets.clone(),
        blocks: blocks.clone(),
    };

    let serialized_index= serialize_index(&temp_index);

    file.write_all(&(serialized_index.len() as u32).to_be_bytes())?;

    file.write_all(&serialized_index)?;

    let footer= SSTableFooter {
        index_offset,
        bloom_offset: 0,
    };

    file.write_all(&footer.index_offset.to_be_bytes())?;

    file.write_all(&footer.bloom_offset.to_be_bytes())?;
    
    Ok(SSTableIndex { offsets, blocks })
}

pub fn read_sstable(path: &str) -> Result<Vec<(String, Value)>> {
    let mut file = File::open(path)?;

    let mut bytes = Vec::new();

    loop {
        // Each block is prefixed with: 4-byte data_len + 4-byte CRC32
        let mut len_buf = [0u8; 4];

        match file.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }

        let data_len = u32::from_be_bytes(len_buf) as usize;

        let mut checksum_buf = [0u8; 4];
        file.read_exact(&mut checksum_buf)?;

        let stored_checksum = u32::from_be_bytes(checksum_buf);

        let mut block = vec![0u8; data_len];

        file.read_exact(&mut block)?;

        let computed_checksum = hash(&block);

        if computed_checksum != stored_checksum {
            println!("CORRUPTED SSTABLE BLOCK DETECTED");
            return Ok(vec![]);
        }

        bytes.extend(block);
    }

    // println!("{:?}", bytes);

    let mut result = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        let record_type = bytes[i];

        i += 1;

        let key_len =
            u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;

        i += 4;

        let val_len =
            u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;

        i += 4;

        let key = String::from_utf8(bytes[i..i + key_len].to_vec()).unwrap();

        i += key_len;

        let val = match record_type {
            1 => {
                let val = String::from_utf8(bytes[i..i + val_len].to_vec()).unwrap();

                i += val_len;

                Value::Data(val)
            }
            0 => {
                i += val_len;
                Value::Tombstone
            }
            _ => panic!("Invalid record type!"),
        };

        result.push((key, val));
    }

    Ok(result)
}

pub fn search_sstable(
    path: &str,
    index: &SSTableIndex,
    key: &str,
) -> Result<Option<(String, Value)>> {
    // println!("file path: {}", path);

    let mut file = File::open(path)?;

    let block = match find_block(index, key) {
        Some(o) => o,
        None => return Ok(None),
    };

    let record_offset = match block.record_offset.get(key) {
        Some(offset) => *offset,
        None => return Ok(None),
    };

    println!("reading block at offset: {}", record_offset);

    file.seek(SeekFrom::Start(record_offset))?;

    let mut type_buf = [0u8; 1];
    file.read_exact(&mut type_buf)?;

    let record_type = type_buf[0];

    let mut len_buff = [0u8; 4];

    file.read_exact(&mut len_buff)?;
    let key_len = u32::from_be_bytes(len_buff) as usize;

    file.read_exact(&mut len_buff)?;
    let val_len = u32::from_be_bytes(len_buff) as usize;

    let mut key_buff = vec![0u8; key_len];
    file.read_exact(&mut key_buff)?;

    let found_key = String::from_utf8(key_buff).unwrap();

    let value = match record_type {
        1 => {
            let mut val_buf = vec![0u8; val_len];
            file.read_exact(&mut val_buf)?;

            Value::Data(String::from_utf8(val_buf).unwrap())
        }

        0 => {
            if val_len > 0 {
                let mut skip = vec![0u8; val_len];
                file.read_exact(&mut skip)?;
            }
            Value::Tombstone
        }

        _ => panic!("Invalid record type"),
    };

    Ok(Some((found_key, value)))
}

pub fn find_block<'a>(index: &'a SSTableIndex, key: &str) -> Option<&'a BlockMeta> {
    let mut candidate = None;

    println!("BLOCKS: {:?}", index.blocks);

    for block in &index.blocks {
        if key >= block.start_key.as_str() {
            candidate = Some(block);
        } else {
            break;
        }
        println!(
            "find_block: key = {}, start_key = {}, chosen offset = {:?}",
            key,
            block.start_key.as_str(),
            candidate
        );
    }

    println!("find_block: final chosen offset = {:?}", candidate);
    candidate
}

pub fn read_block(path: &str, offset: u64) -> Result<Vec<(String, Value)>> {
    let mut file = File::open(path)?;

    // Each block starts with: 4-byte data_len + 4-byte CRC32 + data
    // offset points to the start of the header
    file.seek(SeekFrom::Start(offset))?;

    let mut len_buf = [0u8; 4];
    file.read_exact(&mut len_buf)?;
    let data_len = u32::from_be_bytes(len_buf) as usize;

    // Skip the 4-byte CRC32
    let mut checksum_buf = [0u8; 4];
    file.read_exact(&mut checksum_buf)?;

    let stored_checksum = u32::from_be_bytes(checksum_buf);

    let mut block = vec![0u8; data_len];
    file.read_exact(&mut block)?;

    let computed_checksum = hash(&block);

    if computed_checksum != stored_checksum {
        println!("CORRUPTED SSTABLE BLOCK DETECTED");
        return Ok(vec![]);
    }

    let mut result = vec![];

    let mut i = 0;
    while i < block.len() {
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
                i += val_len;

                Value::Tombstone
            }

            _ => break,
        };

        result.push((key, value));
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
        blocks.push(BlockMeta {
            start_key,
            offset,
            record_offset: BTreeMap::new(),
        });
    }
    SSTableIndex {
        offsets: BTreeMap::new(),
        blocks,
    }
}

pub fn read_footer(path: &str) -> Result<SSTableFooter> {
    let mut file= File::open(path)?;

    let file_size= file.metadata()?.len();

    // footer is last 16 bytes
    file.seek(SeekFrom::Start(file_size - 16))?;

    let mut buf= [0u8; 8];

    file.read_exact(&mut buf)?;

    let index_offset= u64::from_be_bytes(buf);

    file.read_exact(&mut buf)?;

    let bloom_offset= u64::from_be_bytes(buf);

    Ok(SSTableFooter { index_offset, bloom_offset })
}

pub fn load_index_from_footer(path: &str) -> Result<SSTableIndex> {
    let footer= read_footer(path)?;

    let mut file= File::open(path)?;

    file.seek(SeekFrom::Start(footer.index_offset))?;

    let mut len_buf= [0u8; 4];

    file.read_exact(&mut len_buf)?;

    let len= u32::from_be_bytes(len_buf) as usize;

    let mut bytes= vec![0u8; len];

    file.read_exact(&mut bytes)?;

    Ok(deserialize_index(&bytes))
}