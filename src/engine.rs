use std::{clone, collections::{BTreeMap, HashMap}};

use crate::{command::Command, sstable::write_sstable};


pub struct Engine {
    memtable: BTreeMap<String, String>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            memtable: BTreeMap::new(),
        }
    }

    pub fn execute(&mut self, command:Command) -> Option<String> {
        match command {
            Command::Set(key, val) => {
                self.memtable.insert(key, val);
                Some("OK".to_string())
            }
            Command::Get(key) => {
                self.memtable.get(&key).cloned().or(Some("Key not found!".to_string()))
            }
            Command::Del(key) => {
                self.memtable.remove(&key);
                Some("Deleted".to_string())
            }
            Command::Exit => {
                Some("Bye!".to_string())
            }
            Command::Invalid => {
                Some("Invalid command!".to_string())
            }
        }
    }

    pub fn snapshot(&self)-> Vec<(String, String)> {
        self.memtable.iter().map(|(k,v)| (k.clone(), v.clone())).collect()
    }

    pub fn flush_to_sstable(&mut self, path:&str) {
        let data= self.snapshot();

        write_sstable(path, &data).expect("Failed to write SSTable");

        self.memtable.clear();
    }
}
