use std::{cmp::Ordering, collections::{BTreeMap, BinaryHeap, HashMap}};

use bloom::{ASMS, BloomFilter};

use crate::{command::Command, error::Result, sstable::{read_sstable, search_sstable, write_sstable}, sstable_manager::{SSTable, SSTableManager, discover_sstables}};


#[derive(Clone, Debug)]
pub enum Value {
    Data(String),
    Tombstone,
}

pub struct Engine {
    memtable: BTreeMap<String, Value>,
    pub(crate) sstables: SSTableManager,
}

#[derive(Clone, Debug)]
pub struct HeapItem {
    key: String,
    val: Value,
    source_idx:usize, 
}

const MEMTABLE_LIMIT:usize= 1000;

impl Eq for HeapItem {}

impl PartialEq for HeapItem {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        other.key.cmp(&self.key)
    }
}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
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
            Command::Scan(start, end) => {
                let result= self.scan(&start, &end);

                Some(format!("{:?}", result))
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

    pub fn scan(&self, start:&str, end:&str) -> Vec<(String, Value)> {
        let mut sources:Vec<Vec<(String, Value)>>= Vec::new();

        // memtable (already sorted)
        let mem_data:Vec<(String, Value)>= self.memtable.iter().map(|(k,v)| (k.clone(), v.clone())).collect();

        // println!("data: {:?}", mem_data);

        sources.push(mem_data);

        // SSTable
        for table in &self.sstables.tables {
            // println!("{:?}", table.path);
            let data= read_sstable(&table.path).expect("Scan Failed!");
            sources.push(data);
        }

        let mut heap= BinaryHeap::new();

        let mut positions= vec![0usize; sources.len()];

        // println!("{:?}", sources);

        for (src_idx, source) in sources.iter().enumerate() {
            // println!("{:?} source:{:?} data: {:?}", src_idx, source, source.get(1));

            if let Some((k,v))= source.get(0) {
                heap.push(HeapItem {
                    key: k.clone(),
                    val: v.clone(),
                    source_idx: src_idx,
                });
            }
        }

        let mut merged: HashMap<String, Value>= HashMap::new();

        // println!("{:?}", heap);

        while let Some(item)= heap.pop() {
            // println!("{:?}", item);
            if item.key.as_str() >= start && item.key.as_str() < end {
                merged.insert(item.key.clone(), item.val.clone());
            }
            let src= item.source_idx;

            positions[src] += 1;

            // println!("sources: {:?} positions: {:?}", sources, positions);
            // println!("{:?}", sources[src].get(positions[src]));

            if let Some((k,v)) = sources[src].get(positions[src]) {
                heap.push(HeapItem { key: k.clone(), val: v.clone(), source_idx: src });
            }
        }

        let mut result:Vec<_>= merged.into_iter().filter(|(_,v)| {
            matches!(v, Value::Data(_))
        }).collect();

        result.sort_by(|a,b| a.0.cmp(&b.0));

        result
    }
    
}

