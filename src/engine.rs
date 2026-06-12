use std::{cmp::Ordering, collections::{BTreeMap, BinaryHeap, HashMap}, fs, sync::{Arc, Mutex, mpsc::{self, Sender}}, thread};

use bloom::{ASMS, BloomFilter};

use crate::{cache::CacheKey, command::Command, error::Result, sstable::{find_block, read_block, read_sstable, search_sstable, write_sstable}, sstable_manager::{Level, SSTable, SSTableManager, next_sstable_id}};
use crate::cache::BlockCache;


#[derive(Clone, Debug)]
pub enum Value {
    Data(String),
    Tombstone,
}

pub struct Engine {
    pub memtable: BTreeMap<String, Value>,
    pub sstables: Arc<Mutex<SSTableManager>>,
    pub memtable_limit: usize,
    pub compaction_tx: Sender<()>,
    pub block_cache: BlockCache,
}

#[derive(Clone, Debug)]
pub struct HeapItem {
    key: String,
    val: Value,
    source_idx:usize, 
}

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
        let (tx, rx)= mpsc::channel::<()>();

        let shared_sstables= Arc::new(Mutex::new(SSTableManager::new()));

        let worker_sstables= Arc::clone(&shared_sstables);

        thread::spawn(move || {
            while let Ok(_) = rx.recv() {
                println!("[Background Worker] Compaction started");

                let mut sstables= worker_sstables.lock().unwrap();

                if let Err(e)= sstables.maybe_compact() {
                    println!("[Background Worker] Compaction error: {}",e);
                }

                    println!("[Background Worker] Compaction finished");

            }
        });

        Self {
            memtable: BTreeMap::new(),
            sstables: Arc::new(Mutex::new(SSTableManager::new())),
            memtable_limit: 1000,
            compaction_tx: tx,
            block_cache: BlockCache::new(64), // 64 blocks
        }
    }

    pub fn execute(&mut self, command:Command) -> Option<String> {
        match command {
            Command::Set(key, val) => {
                self.memtable.insert(key, Value::Data(val));

                self.maybe_flush().expect("Flush failed!");


                Some("OK".to_string())
            }
            Command::Get(key) => {
                self.get_key(&key).map(|v| match v {
                    Value::Data(d) => {
                        d                       
                    },
                    Value::Tombstone => "Key not found!".to_string(),
                })
            }
            Command::Del(key) => {
                self.memtable.insert(key, Value::Tombstone);

                self.maybe_flush().expect("Flush failed!");

                Some("Deleted".to_string())
            }
            Command::Exit => {
                if self.memtable_size() > 0 {
                    let sstable_id = next_sstable_id();
                    let file = format!("sst_l0_{}.bin", sstable_id);
                    if let Err(e) = self.flush_to_sstable(&file) {
                        return Some(format!("flush failed: {}", e));
                    }
                }
                
                let mut sstables = self.sstables.lock().unwrap();
                match sstables.compact() {
                    Ok(_) => Some("Bye!".to_string()),
                    Err(e) => Some(format!("compaction failed: {}", e)),
                }
                
            }
            Command::Compact => {
                // Flush memtable to SSTable first, then compact
                if self.memtable_size() > 0 {
                    let sstable_id = next_sstable_id();
                    let file = format!("sst_l0_{}.bin", sstable_id);
                    if let Err(e) = self.flush_to_sstable(&file) {
                        return Some(format!("flush failed: {}", e));
                    }
                }
                let mut sstables = self.sstables.lock().unwrap();
                match sstables.compact() {
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
        if data.is_empty() {
            // skip SSTable creation
            return Ok(()); 
        }

        let size = data.len().max(8) as u32;
        let mut bloom = BloomFilter::with_rate(0.01, size);
        

        for (key, _) in &data {
            bloom.insert(key);
        }
        // println!("{:?}", path);
        let index=  write_sstable(path, &data)?;

        let min_key= data.first().map(|(k, _)|k.clone()).unwrap();

        let max_key= data.last().map(|(k, _)|k.clone()).unwrap();

        let file_size= fs::metadata(&path)?.len();

        {
            let mut sstables = self.sstables.lock().unwrap();
            sstables.add_table(
                SSTable {
                    path: path.to_string(),
                    index,
                    bloom,
                    level: Level::L0,
                    max_key,
                    min_key,
                    file_size
                }
            );
        }

        let mut sstables = self.sstables.lock().unwrap();

        if sstables.l0.len() >= 4 {
            sstables.size_tiered_compact_l0()?;
        }

        // sstables.maybe_compact()?;
        
        self.memtable.clear();

        let _= self.compaction_tx.send(());

        Ok(())
    }

    pub fn get_key(&mut self, key:&str) -> Option<Value> {

        println!("GET KEY: {:?}", key);
        if let Some(val)= self.memtable.get(key) {
            // println!("{:?}", val);
            return Some(val.clone());
        }
        let (l0, l1, l2) = {

            let sstables =self.sstables.lock().unwrap();
            (
                sstables.l0.clone(),
                sstables.l1.clone(),
                sstables.l2.clone(),
            )
        };

        if let Some(v) = self.search_level(&l0, key) {
            return Some(v);
        }

        if let Some(v) = self.search_level(&l1, key) {
            return Some(v);
        }   

        if let Some(v) = self.search_level(&l2, key) {
            return Some(v);
        }

        Some(Value::Tombstone)
    }

    pub fn search_level(&mut self, level: &[SSTable], key:&str) -> Option<Value> {
        for table in level.iter().rev() {
            println!("table index: {:?}", table.index);

            if !table.contains_key_range(key) {
                continue;
            }
            
            println!("bloom check: {} -> {}", key, table.bloom.contains(&key));

            if !table.bloom.contains(&key) {
                continue;
            }

            println!("checking SSTable: {}", table.path);

            let block= match find_block(&table.index, key) {
                Some(block) => block,
                None => continue,
            };

            let cache_key= CacheKey {
                path: table.path.clone(),
                offset: block.offset,
            };

            if let Some(records)= self.block_cache.get(&cache_key) {
                println!("CACHE HIT: {:?}", cache_key);

                for (k,v) in records {
                    if k == key {
                        if let Value::Tombstone = v {
                            return Some(Value::Tombstone);
                        }
                        return Some(v);
                    }
                }

                continue;
            }

            println!("CACHE MISS: {:?}", cache_key);

            let records= match read_block(&table.path, block.offset) {
                Ok(records) => records,
                Err(e) => {
                    println!("BLOCK READ ERROR: {:?}", e);
                    continue;
                }
            };

            self.block_cache.insert(cache_key, records.clone());

            for (k,v) in records {
                if k == key {
                        if let Value::Tombstone = v {
                            return Some(Value::Tombstone);
                        }
                    return Some(v);
                }
            }
        }

        None

    }

    pub fn maybe_flush(&mut self) -> Result<()> {

        if self.memtable.len() >= self.memtable_limit {
            let path = format!("sst_l0_{}.bin",next_sstable_id());
    
            self.flush_to_sstable(&path)?;
        }

        Ok(())
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

        let sstables = self.sstables.lock().unwrap();
        // SSTable
        for table in &sstables.l0 {
            // println!("{:?}", table.path);
            let data= read_sstable(&table.path).expect("Scan of L0 Failed!");
            sources.push(data);
        }

        for table in &sstables.l1 {
            let data = read_sstable(&table.path).expect("Scan of L1 failed");
            sources.push(data);
        }

        for table in &sstables.l2 {
            let data = read_sstable(&table.path).expect("Scan of L2 failed");
            sources.push(data);
        }

        let mut heap= BinaryHeap::new();

        let mut positions= vec![0usize; sources.len()];

        // println!("{:?}", sources);

        for (src_idx, source) in sources.iter().enumerate() {
            // println!("{:?} source:{:?} data: {:?}", src_idx, source, source.get(1));

            if let Some((k,v))= source.first() {
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
                merged.entry(item.key.clone()).or_insert(item.val.clone());
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
