use std::{collections::BTreeMap};

use crate::{command::Command, sstable::{search_sstable, write_sstable}, sstable_manager::{SSTable, SSTableManager}};


#[derive(Clone, Debug)]
pub enum Value {
    Data(String),
    Tombstone,
}

pub struct Engine {
    memtable: BTreeMap<String, Value>,
    pub(crate) sstables: SSTableManager,
}


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
                Some("Deleted".to_string())
            }
            Command::Exit => {
                Some("Bye!".to_string())
            }
            Command::Compact => {
                self.sstables.compact();
                Some("Compaction completed!".to_string())
            }

            Command::Invalid => {
                Some("Invalid command!".to_string())
            }
        }
    }

    pub fn snapshot(&self)-> Vec<(String, Value)> {
        self.memtable.iter().map(|(k,v)| (k.clone(), v.clone())).collect()
    }

    pub fn flush_to_sstable(&mut self, path:&str) {
        let data= self.snapshot();

        // println!("{:?}", path);
        let index=  write_sstable(path, &data).expect("Failed to write SSTable");

        self.sstables.tables.push(
            SSTable {
                path: path.to_string(),
                index
            }
        );
        self.memtable.clear();
    }

    pub fn get_key(&self, key:&str) -> Option<Value> {
        if let Some(val)= self.memtable.get(key) {
            println!("{:?}", val);
            return Some(val.clone());
        }


        for table in self.sstables.tables.iter().rev() {
            if let Some(offset)= table.index.offsets.get(key) {
                let (_,val) = search_sstable(&table.path, *offset).expect("Failed to read!");

                println!("{:?}", val);
                return Some(val);
            }
        }

        // println!("Key not found!");
        Some(Value::Tombstone)
    }
}
