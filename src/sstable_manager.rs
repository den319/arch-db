use std::{collections::{BTreeMap, HashMap}, fs};

use crate::{engine::Value, sstable::{SSTableIndex, read_sstable, write_sstable}};


pub struct SSTableManager {
    pub(crate) tables: Vec<SSTable>,
}

#[derive(Debug)]
pub struct SSTable {
    pub path: String,
    pub index: SSTableIndex,
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

        let mut offset= 0u64;

        for (key, val) in &data {
            offsets.insert(key.clone(), offset);

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
            index: SSTableIndex { offsets },
        };

        // println!("{:?}", table);

        self.tables.push(table);
     }

    pub fn compact(&mut self) {
        let mut merged= HashMap::new();

        for table in &self.tables {
            let data= read_sstable(&table.path).expect("Failed to read data!");

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

        let index= write_sstable("sst_compacted.bin", &sorted).expect("Compaction failed!");

        for table in &self.tables {
            fs::remove_file(&table.path).expect("Failed to delete old SSTable");
        }
        self.tables.clear();


        self.tables.push(SSTable { path: "sst_compacted.bin".to_string(), index });
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
                max_id= max_id.max(id+1);
            }
        }
    }        

    max_id
}