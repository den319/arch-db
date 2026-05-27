use std::{collections::BTreeMap};

use bloom::{ASMS, BloomFilter};

use crate::{command::Command, error::Result, sstable::{search_sstable, write_sstable}, sstable_manager::{SSTable, SSTableManager, discover_sstables}};


#[derive(Clone, Debug)]
pub enum Value {
    Data(String),
    Tombstone,
}

pub struct Engine {
    memtable: BTreeMap<String, Value>,
    pub(crate) sstables: SSTableManager,
}

const MEMTABLE_LIMIT:usize= 1000;


impl Engine {
    pub fn new() -> Self {
        Self {
            memtable: BTreeMap::new(),
            sstables: SSTableManager::new(),
        }
    }

    pub fn execute(&mut self, command:Command) -> Option<String> {
        match command {
            Command::Set(key, val) => {
                self.memtable.insert(key, Value::Data(val));

                let sstable_id= discover_sstables();
                // println!("{:?}", sstable_id);
                
                if self.memtable_size() >= MEMTABLE_LIMIT {
                    

                    let file= format!("sst_{}.bin", sstable_id);

                    let _= self.flush_to_sstable(&file);
                }

                Some("OK".to_string())
            }
            Command::Get(key) => {
                self.get_key(&key).map(|v| match v {
                    Value::Data(d) => {
                        return d;                        
                    },
                    Value::Tombstone => "Key not found!".to_string(),
                })
            }
            Command::Del(key) => {
                self.memtable.insert(key, Value::Tombstone);

                if self.memtable_size() >= MEMTABLE_LIMIT {
                    let sstable_id= discover_sstables();

                    let file= format!("sst_{}.bin", sstable_id);
                    let _= self.flush_to_sstable(&file);
                }

                Some("Deleted".to_string())
            }
            Command::Exit => {
                Some("Bye!".to_string())
            }
            Command::Compact => {
                match self.sstables.compact() {
                    Ok(_) => Some("Compaction completed!".to_string()),
                    Err(e) => Some(format!("compaction failed: {}", e)),
                }
                
            }

            Command::Invalid => {
                Some("Invalid command!".to_string())
            }
        }
    }

    pub fn snapshot(&self)-> Vec<(String, Value)> {
        self.memtable.iter().map(|(k,v)| (k.clone(), v.clone())).collect()
    }

    pub fn flush_to_sstable(&mut self, path:&str) -> Result<()> {
        let data= self.snapshot();

        let mut bloom = BloomFilter::with_rate(0.01,data.len() as u32);

        for (key, _) in &data {
            bloom.insert(key);
        }
        // println!("{:?}", path);
        let index=  write_sstable(path, &data)?;

        self.sstables.tables.push(
            SSTable {
                path: path.to_string(),
                index,
                bloom
            }
        );
        self.memtable.clear();

        Ok(())
    }

    pub fn get_key(&self, key:&str) -> Option<Value> {

        if let Some(val)= self.memtable.get(key) {
            // println!("{:?}", val);
            return Some(val.clone());
        }


        for table in self.sstables.tables.iter().rev() {
            if !table.bloom.contains(&key) {
                println!("{:?}", key);
                continue;
            }

            if let Some(offset)= table.index.offsets.get(key) {
                let (_,val) = search_sstable(&table.path, *offset).expect("Failed to read!");

                // println!("{:?}", val);
                return Some(val);
            }
        }

        // println!("Key not found!");
        Some(Value::Tombstone)
    }

    pub fn memtable_size(&self) -> usize {
        self.memtable.len()
    }
}

