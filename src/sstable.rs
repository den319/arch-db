use std::{collections::BTreeMap, fs::File, io::{Read, Seek, SeekFrom, Write}};

use crate::{engine::Value, error::Result};

#[derive(Debug)]
pub struct BlockMeta {
    pub start_key: String,
    pub offset: u64,
    pub record_offset: BTreeMap<String, u64>,
}
#[derive(Debug)]
pub struct SSTableIndex {
    pub offsets: BTreeMap<String, u64>,
    pub blocks: Vec<BlockMeta>,
}

pub const BLOCK_SIZE: usize = 40;

pub fn write_sstable(
    path:&str, 
    data: &[(String, Value)]
) -> Result<SSTableIndex> {
    let mut offsets= BTreeMap::new();
    let mut file_offset= 0u64;
    
    let mut current_block_offsets= BTreeMap::new();

    let mut block_size= 0usize;

    let mut single_block= Vec::new();

    let mut blocks:Vec<BlockMeta>= Vec::new();

    let mut file= File::create(path)?;

    let mut is_new_block= true;
    
    println!("data: {:?}", data);

    for (key, val) in data  {

        let mut record= Vec::new();

        record.push(match val {
            Value::Data(_) => 1u8,
            Value::Tombstone => 0u8,
        });

        record.extend(&(key.len() as u32).to_be_bytes());

        let value_bytes= match val {
            Value::Data(v) => v.as_bytes(),
            Value::Tombstone => b"",
        };

        record.extend(&(value_bytes.len() as u32).to_be_bytes());

        record.extend(key.as_bytes());
        record.extend(value_bytes);

        if block_size + record.len() > BLOCK_SIZE {
            file.write_all(&single_block)?;

            file_offset += single_block.len() as u64;

            if let Some(last_block) = blocks.last_mut() {
                last_block.record_offset = current_block_offsets.clone();
            }

            current_block_offsets.clear();
            single_block.clear();
            block_size=0;

            is_new_block= true;
        }

        if is_new_block {
            blocks.push(BlockMeta { start_key: key.clone(), offset: file_offset, record_offset: BTreeMap::new() });
            is_new_block= false;
        }

        let record_offset= file_offset + single_block.len() as u64;

        current_block_offsets.insert(key.clone(), record_offset);

        single_block.extend(&record);
        block_size += record.len();

        offsets.insert(key.clone(), record_offset);

    }

    if !single_block.is_empty() {
        if let Some(last_block)= blocks.last_mut() {
            last_block.record_offset= current_block_offsets.clone();
        }
        file.write_all(&single_block)?;
    }

    blocks.sort_by(|a,b| a.start_key.cmp(&b.start_key));

    Ok(SSTableIndex { offsets, blocks })
}

pub fn read_sstable(path:&str) -> Result<Vec<(String, Value)>> {
    let mut file= File::open(path)?;

    let mut bytes= Vec::new();

    loop {
        let mut block= vec![0u8; BLOCK_SIZE];

        let bytes_read= file.read(&mut block)?;

        if bytes_read == 0 {
            break;
        }

        block.truncate(bytes_read);
        bytes.extend(block);
    }

    // println!("{:?}", bytes);

    let mut result= Vec::new();
    let mut i=0;

    while i < bytes.len() {
        let record_type= bytes[i];

        i += 1;

        let key_len= u32::from_be_bytes([
            bytes[i], bytes[i+1], bytes[i+2], bytes[i+3],
        ]) as usize;

        i += 4;

        let val_len= u32::from_be_bytes([
            bytes[i], bytes[i+1], bytes[i+2], bytes[i+3],
        ]) as usize;

        i+= 4;

        let key= String::from_utf8(bytes[i..i+key_len].to_vec()).unwrap();

        i += key_len;

        let val= match record_type {
            1 => {
                let val= String::from_utf8(bytes[i..i+val_len].to_vec()).unwrap();

                i += val_len;

                Value::Data(val)
            }
            0 => {
                i += val_len;
                Value::Tombstone
            },
            _ => panic!("Invalid record type!")
        };

        result.push((key, val));
    }

    Ok(result)

}

pub fn search_sstable(path: &str, index: &SSTableIndex, key:&str) -> Result<Option<(String, Value)>> {
    let mut file= File::open(path)?;

    let block= match find_block(index, key) {
        Some(o) => o,
        None => return Ok(None),
    };

    let record_offset= match block.record_offset.get(key) {
        Some(offset) => *offset,
        None => return Ok(None)
    };

    println!("reading block at offset: {}", record_offset);

    file.seek(SeekFrom::Start(record_offset))?;

    let mut type_buf = [0u8; 1];
    file.read_exact(&mut type_buf)?;

    let record_type = type_buf[0];

    let mut len_buff= [0u8;4];

    file.read_exact(&mut len_buff)?;
    let key_len= u32::from_be_bytes(len_buff) as usize;

    file.read_exact(&mut len_buff)?;
    let val_len= u32::from_be_bytes(len_buff) as usize;

    let mut key_buff= vec![0u8;key_len];
    file.read_exact(&mut key_buff)?;

    let found_key= String::from_utf8(key_buff).unwrap();

    let value = match record_type {
        1 => {
            let mut val_buf = vec![0u8; val_len];
            file.read_exact(&mut val_buf)?;

            Value::Data(String::from_utf8(val_buf).unwrap())
        }

        0 => {
            if val_len > 0 {
                let mut skip= vec![0u8;val_len];
                file.read_exact(&mut skip)?;
            }
            Value::Tombstone
        },

        _ => panic!("Invalid record type"),
    };

    Ok(Some((found_key, value)))
}

pub fn find_block<'a>(index: &'a SSTableIndex, key:&str) -> Option<&'a BlockMeta> {
    let mut candidate= None;

    println!("BLOCKS: {:?}", index.blocks);

    for block in &index.blocks {
        if key >= block.start_key.as_str() {
            candidate = Some(block);
        } else {
            break;
        }
        println!("find_block: key = {}, start_key = {}, chosen offset = {:?}", key, block.start_key.as_str(), candidate);
    }

    println!("find_block: final chosen offset = {:?}", candidate);
    candidate
}



#[cfg(test)]
mod tests {

    use super::*;
    use std::fs;

    fn sample_data() -> Vec<(String, Value)> {
        vec![
            ("a".to_string(), Value::Data("1".to_string())),
            ("b".to_string(), Value::Data("2".to_string())),
            ("c".to_string(), Value::Data("3".to_string())),
            ("d".to_string(), Value::Data("4".to_string())),
            ("e".to_string(), Value::Data("5".to_string())),
            ("f".to_string(), Value::Data("6".to_string())),
            ("g".to_string(), Value::Data("7".to_string())),
            ("h".to_string(), Value::Tombstone),
        ]
    }

    #[test]
    fn test_write_and_read_sstable() {

        let path = "test_write_and_read_sstable.bin";

        let data = sample_data();

        write_sstable(path, &data).unwrap();

        let loaded = read_sstable(path).unwrap();

        assert_eq!(loaded.len(), data.len());

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_find_existing_key() {

        let path = "test_find_existing_key.bin";

        let data = sample_data();

        let index = write_sstable(path, &data).unwrap();

        let result =
            search_sstable(path, &index, "c")
                .unwrap();

        assert!(result.is_some());

        let (_, value) = result.unwrap();

        match value {
            Value::Data(v) => {
                assert_eq!(v, "3");
            }
            _ => panic!("Expected Data"),
        }

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_find_tombstone() {

        let path = "test_find_tombstone.bin";

        let data = sample_data();

        let index = write_sstable(path, &data).unwrap();

        let result =
            search_sstable(path, &index, "h")
                .unwrap();

        assert!(result.is_some());

        let (_, value) = result.unwrap();

        match value {
            Value::Tombstone => {}
            _ => panic!("Expected Tombstone"),
        }

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_missing_key() {

        let path = "test_missing_key.bin";

        let data = sample_data();

        let index = write_sstable(path, &data).unwrap();

        let result =
            search_sstable(path, &index, "z")
                .unwrap();

        assert!(result.is_none());

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_multiple_blocks_created() {

        let path = "test_multiple_blocks_created.bin";

        let data = sample_data();

        let index = write_sstable(path, &data).unwrap();

        println!("{:?}", index.blocks);

        assert!(index.blocks.len() > 1);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_find_correct_block() {

        let path = "test_find_correct_block.bin";

        let data = sample_data();

        let index = write_sstable(path, &data).unwrap();

        let block =
            find_block(&index, "f");

        assert!(block.is_some());

        let block = block.unwrap();

        assert!(block.start_key <= "f".to_string());

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_record_offsets_exist() {

        let path = "test_record_offsets_exist.bin";

        let data = sample_data();

        let index = write_sstable(path, &data).unwrap();

        let mut found = false;

        for block in &index.blocks {

            if block.record_offset.contains_key("c") {
                found = true;
            }
        }

        assert!(found);

        fs::remove_file(path).unwrap();
    }
}