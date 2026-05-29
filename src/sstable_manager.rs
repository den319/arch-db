use std::{collections::{BTreeMap, HashMap}, fs};

use bloom::{ASMS, BloomFilter};

use crate::{engine::Value, error::Result, sstable::{BLOCK_SIZE, BlockMeta, SSTableIndex, read_sstable, write_sstable}};


pub struct SSTableManager {
    pub(crate) tables: Vec<SSTable>,
}

// #[derive(Debug)]

pub struct SSTable {
    pub path: String,
    pub index: SSTableIndex,
    pub bloom: BloomFilter,
}

impl SSTableManager {
    pub fn new() -> Self {
        Self {
            tables: Vec::new(),
        }
    }

    pub fn load_from_file(&mut self, path: &str) {
        // println!("{}", path);
        let data= read_sstable(path).expect("Failed to read sstable!");

        let mut offsets= BTreeMap::new();
        let mut blocks= Vec::new();

        let mut offset= 0u64;
        let mut current_block_size= 0usize;

        let size = data.len().max(8) as u32;
        let mut bloom= BloomFilter::with_rate(0.01, size);
        if data.is_empty() {
            self.tables.push(SSTable {
                path: path.to_string(),
                index: SSTableIndex { offsets, blocks },
                bloom,
            });
            return;
        }

        for (key, val) in &data {

            let record_size= 1 + 4 + 4 + key.len() + match val {
                Value::Data(v) => v.len(),
                Value::Tombstone => 0,
            };
            
            bloom.insert(&key);

            if current_block_size == 0 {
                blocks.push(BlockMeta {
                    start_key: key.clone(),
                    offset: offset,
                    record_offset: BTreeMap::new(),
                });
            }

            offsets.insert(key.clone(), offset);

            current_block_size += record_size;

            if current_block_size >= BLOCK_SIZE {
                current_block_size = 0;
            }

            // println!("{}", offset);
            offset += match val {
                Value::Data(v) => {
                    1 + 8 + key.len() as u64 + v.len() as u64
                }
                Value::Tombstone => {
                    1 + 8 + key.len() as u64
                }
            }
            
        }

        let table= SSTable {
            path: path.to_string(),
            index: SSTableIndex { offsets, blocks },
            bloom,
        };

        // println!("{:?}", table);

        self.tables.push(table);
     }

    pub fn compact(&mut self) -> Result<()> {
        let mut merged= HashMap::new();


        for table in &self.tables {
            let data= read_sstable(&table.path)?;

            for (key, val) in data {
                match val {
                    Value::Data(v) => {
                        merged.insert(key, Value::Data(v));
                    }
                    Value::Tombstone => {
                        merged.remove(&key);
                    }
                }
            }
        }

        let mut sorted:Vec<(String, Value)>= merged.into_iter().collect();

        sorted.sort_by(|a,b| a.0.cmp(&b.0));

        for table in &self.tables {
            fs::remove_file(&table.path)?;
        }
        self.tables.clear();
        
        if sorted.is_empty() {
            return Ok(());
        }

        let mut bloom = BloomFilter::with_rate(0.01, sorted.len() as u32);

        for (key, _) in &sorted {
            bloom.insert(key);
        }

        let index= write_sstable("sst_compacted.bin", &sorted)?;

        self.tables.push(SSTable { path: "sst_compacted.bin".to_string(), index, bloom });

        Ok(())
    }
}


pub fn discover_sstables() -> usize {
    let mut max_id=0;

    let entries= fs::read_dir(".").expect("Failed to read directory!");

    for entry in entries {
        let entry= entry.unwrap();
        
        let name= entry.file_name();
        let name= name.to_string_lossy();

        if name.starts_with("sst_") && name.ends_with(".bin") {
            let id_part= name.trim_start_matches("sst_").trim_end_matches(".bin");
            
            if let Ok(id)= id_part.parse::<usize>() {
                max_id= max_id.max(id);
            }
        }
    }        

    max_id + 1
}
