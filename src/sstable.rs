use std::{collections::BTreeMap, fs::File, io::{Read, Seek, SeekFrom, Write}};

use crate::{engine::Value, error::Result};

#[derive(Debug)]
pub struct SSTableIndex {
    pub offsets: BTreeMap<String, u64>,
    pub blocks: Vec<(String, u64)>,
}

pub const BLOCK_SIZE:usize= 4096;

pub fn write_sstable(
    path:&str, 
    data: &[(String, Value)]
) -> Result<SSTableIndex> {
    let mut offsets= BTreeMap::new();
    let mut file_offset= 0u64;
    let mut block_start_offset= 0u64;
    let mut block_size= 0usize;

    let mut single_block= Vec::new();

    let mut blocks= Vec::new();

    let mut file= File::create(path)?;

    let mut is_new_block= true;
    
    println!("data: {:?}", data);

    for (key, val) in data  {

        let record_start_in_file= file_offset + block_size as u64;

        if is_new_block {
            blocks.push((key.clone(), record_start_in_file));
            is_new_block= false;
        }

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

            single_block.clear();
            block_size=0;

            is_new_block= true;
        }

        single_block.extend(&record);
        block_size += record.len();

        offsets.insert(key.clone(), record_start_in_file);

    }

    if !single_block.is_empty() {
        file.write_all(&single_block)?;
    }

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
            0 => Value::Tombstone,
            _ => panic!("Invalid record type!")
        };

        result.push((key, val));
    }

    Ok(result)

}

pub fn search_sstable(path: &str, index: &SSTableIndex, key:&str) -> Result<Option<(String, Value)>> {
    let mut file= File::open(path)?;

    let block_offset= match find_block(index, key) {
        Some(o) => o,
        None => return Ok(None),
    };

    file.seek(SeekFrom::Start(block_offset))?;

    let mut buffer= vec![0u8; BLOCK_SIZE];
    let bytes_read= file.read(&mut buffer)?;

    buffer.truncate(bytes_read);

    let mut i=0;

    while i < buffer.len() {
        if i + 9 > buffer.len() {
            break;
        }

        let record_type= buffer[i];
        i += 1;

        let key_len = u32::from_be_bytes([buffer[i], buffer[i+1], buffer[i+2], buffer[i+3]]) as usize;
        i += 4;

        let val_len = u32::from_be_bytes([buffer[i], buffer[i+1], buffer[i+2], buffer[i+3]]) as usize;
        i += 4;

        if i + key_len > buffer.len() {
            break;
        }

        let k = String::from_utf8(buffer[i..i+key_len].to_vec()).unwrap();
        i += key_len;

        let v= match record_type {
            1 => {
                if i + val_len > buffer.len() {
                    break;
                }
                let val= String::from_utf8(buffer[i..i+val_len].to_vec()).unwrap();
                i += val_len;
                Value::Data(val)
            }
            0 => {
                i += val_len;
                Value::Tombstone
            },
            _ => panic!("Invalid record type"),
        };

        if k == key {
            return Ok(Some((k,v)));
        }
    }


    Ok(None)

}

pub fn find_block(index: &SSTableIndex, key:&str) -> Option<u64> {
    let mut candidate= None;

    for(start_key, offset) in &index.blocks {
        if key >= start_key {
            candidate = Some(*offset);
        } else {
            break;
        }
    }

    candidate
}