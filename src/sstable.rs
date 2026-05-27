use std::{collections::BTreeMap, fs::File, io::{Read, Seek, SeekFrom, Write}};

use crate::{engine::Value, error::Result};

#[derive(Debug)]
pub struct SSTableIndex {
    pub offsets: BTreeMap<String, u64>,
}

pub fn write_sstable(
    path:&str, 
    data: &[(String, Value)]
) -> Result<SSTableIndex> {
    let mut offsets= BTreeMap::new();
    let mut cursor_offset= 0u64;

    let mut file= File::create(path)?;
    
    for (key, val) in data  {
        offsets.insert(key.clone(), cursor_offset);

        match val {
            Value::Data(v) => {
                file.write_all(&[0u8])?;
                file.write_all(&(key.len() as u32).to_be_bytes())?;
                file.write_all(&(v.len() as u32).to_be_bytes())?;
                file.write_all(key.as_bytes())?;
                file.write_all(v.as_bytes())?;
        
                cursor_offset += 1 + 4 + 4 + key.len() as u64 + v.len() as u64;
            }
            Value::Tombstone => {
                file.write_all(&[1u8])?;
                file.write_all(&(key.len() as u32).to_be_bytes())?;
                file.write_all(&0u32.to_be_bytes())?;
                file.write_all(key.as_bytes())?;

                cursor_offset += 1 + 4 + 4 + key.len() as u64;
            }
        }


    }

    Ok(SSTableIndex { offsets })
}

pub fn read_sstable(path:&str) -> Result<Vec<(String, Value)>> {
    let mut file= File::open(path)?;

    let mut bytes= Vec::new();

    file.read_to_end(&mut bytes)?;

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
            0 => {
                let val= String::from_utf8(bytes[i..i+val_len].to_vec()).unwrap();

                i += val_len;

                Value::Data(val)
            }
            1 => Value::Tombstone,
            _ => panic!("Invalid record type!")
        };



        result.push((key, val));
    }

    Ok(result)

}

pub fn search_sstable(path: &str, offset: u64) -> Result<(String, Value)> {
    let mut file= File::open(path)?;

    file.seek(SeekFrom::Start(offset))?;

    let mut len_buff= [0u8;4];

    file.read_exact(&mut len_buff)?;
    let key_len= u32::from_be_bytes(len_buff) as usize;

    file.read_exact(&mut len_buff)?;
    let val_len= u32::from_be_bytes(len_buff) as usize;

    let mut key_buff= vec![0u8; key_len];
    file.read_exact(&mut key_buff)?;

    let mut val_buff= vec![0u8;val_len];
    file.read_exact(&mut val_buff)?;

    let key= String::from_utf8(key_buff).unwrap();
    let val= String::from_utf8(val_buff).unwrap();

    Ok((key, Value::Data(val)))

}