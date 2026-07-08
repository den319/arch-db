use std::{
    cmp::Ordering,
    collections::{BTreeMap, BinaryHeap, HashMap},
    fs,
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    thread,
};

use crate::{cache::BlockCache, engine_iterator::EngineIterator, memtable_iterator::MemtableIterator, merge_iterator::MergeIterator, sstable::SSTableIterator, storage::{Storage, SyncPolicy}, storage_iterator::StorageIterator, unified_storage_iterator::UnifiedStorageIterator};
use crate::{
    bloom_filter::BloomFilter,
    cache::CacheKey,
    command::Command,
    compaction_picker::CompactionPicker,
    error::Result,
    sstable::{binary_search_block, find_block, read_block, read_sstable, write_sstable},
    sstable_manager::{Level, SSTable, SSTableManager, next_sstable_id},
};

#[derive(Clone, Debug, Eq, PartialEq)]
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
    pub storage: Storage,
}

#[derive(Clone, Debug)]
struct ScanHeapItem {
    key: String,
    val: Value,
    source_idx: usize,
}

impl Eq for ScanHeapItem {}

impl PartialEq for ScanHeapItem {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Ord for ScanHeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        other.key.cmp(&self.key)
    }
}

impl PartialOrd for ScanHeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Engine {
    pub fn new() -> Self {
        Self::with_storage_path("storage/temp")
    }

    pub fn with_storage_path(path: &str) -> Self {
        let (tx, rx) = mpsc::channel::<()>();

        let shared_sstables = Arc::new(Mutex::new(SSTableManager::new()));

        let worker_sstables = Arc::clone(&shared_sstables);

        thread::spawn(move || {
            while let Ok(_) = rx.recv() {
                println!("[Background Worker] Compaction started");

                let mut sstables = worker_sstables.lock().unwrap();

                if let Err(e) = sstables.maybe_compact() {
                    println!("[Background Worker] Compaction error: {}", e);
                }

                println!("[Background Worker] Compaction finished");
            }
        });

        let mut engine = Self {

            memtable: BTreeMap::new(),
            sstables: shared_sstables,
            memtable_limit: 1000,
            compaction_tx: tx,
            block_cache: BlockCache::new(64),

            storage: Storage::new(
                path,
                SyncPolicy::Always,
            )
            .expect("Failed to initialize WAL"),
        };

        engine
            .replay_wal()
            .expect("Failed to replay WAL");

        engine
    }

    pub fn put(&mut self, key: String, value: String) -> Result<()> {

        // write-ahead log
        self.storage.append(
            &Command::Set(
                key.clone(),
                value.clone(),
            ),
        )?;

        self.memtable.insert(key, Value::Data(value));

        self.maybe_flush()?;

        Ok(())
    }

    pub fn get(&mut self, key: &str) -> Option<Value> {
        self.get_key(key)
    }

    pub fn delete(&mut self, key: String) -> Result<()> {

        // WAL
        self.storage.append(
            &Command::Del(key.clone()),
        )?;

        self.memtable.insert(key, Value::Tombstone);

        self.maybe_flush()?;

        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<()> {

        if self.memtable.is_empty() {
            return Ok(());
        }

        let file = format!(
            "sst_l0_{}.bin",
            next_sstable_id()
        );

        self.flush_to_sstable(&file)?;

        self.storage.reset()?;

        self.storage.checkpoint()?;

        Ok(())
    }

    pub fn execute(&mut self, command: Command) -> Option<String> {
        match command {
            Command::Set(key, val) => match self.put(key, val) {
                Ok(_) => Some("OK".to_string()),
                Err(e) => Some(format!("put failed: {}", e)),
            },
            Command::Get(key) => self.get(&key).map(|v| match v {
                Value::Data(d) => d,
                Value::Tombstone => "Key not found!".to_string(),
            }),
            Command::Del(key) => match self.delete(key) {
                Ok(_) => Some("Deleted".to_string()),
                Err(e) => Some(format!("delete failed: {}", e)),
            },
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
                let result = self.scan(&start, &end);

                Some(format!("{:?}", result))
            }

            Command::Invalid => Some("Invalid command!".to_string()),
        }
    }

    pub fn snapshot(&self) -> Vec<(String, Value)> {
        self.memtable
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub fn flush_to_sstable(&mut self, path: &str) -> Result<()> {
        let data = self.snapshot();
        if data.is_empty() {
            // skip SSTable creation
            return Ok(());
        }

        let size = data.len().max(8) as u32;
        let mut bloom = BloomFilter::with_rate(0.01, size);

        for (key, _) in &data {
            bloom.insert(key);
        }
        let index = write_sstable(path, &data)?;

        let min_key = data.first().map(|(k, _)| k.clone()).unwrap();
        let max_key = data.last().map(|(k, _)| k.clone()).unwrap();
        let file_size = fs::metadata(&path)?.len();

        {
            let mut sstables = self.sstables.lock().unwrap();
            sstables.add_table(SSTable {
                path: path.to_string(),
                index,
                bloom,
                level: Level::L0,
                max_key,
                min_key,
                file_size,
            });
        }

        let mut sstables = self.sstables.lock().unwrap();
        if let Some(candidate) = CompactionPicker::pick_l0(&sstables) {
            sstables.size_tiered_compact_l0(&candidate)?;
        }

        self.memtable.clear();
        let _ = self.compaction_tx.send(());

        Ok(())
    }

    /// Lock briefly to check bloom filters (fast, memory-only) and
    /// collect candidates. Then release the lock before any disk I/O,
    /// so the background compaction worker is not blocked.
    pub fn get_key(&mut self, key: &str) -> Option<Value> {
        println!("GET KEY: {:?}", key);
        if let Some(val) = self.memtable.get(key) {
            return Some(val.clone());
        }

        // Lock briefly → check bloom (memory only) → collect candidates
        let candidates = {
            let sstables = self.sstables.lock().unwrap();

            let mut v: Vec<(String, crate::sstable::SSTableIndex)> = Vec::new();
            for t in sstables.l0.iter().rev() {
                if !t.contains_key_range(key) {
                    continue;
                }
                if t.bloom.contains(&key) {
                    v.push((t.path.clone(), t.index.clone()));
                }
            }
            for t in sstables.l1.iter().rev() {
                if !t.contains_key_range(key) {
                    continue;
                }
                if t.bloom.contains(&key) {
                    v.push((t.path.clone(), t.index.clone()));
                }
            }
            for t in sstables.l2.iter().rev() {
                if !t.contains_key_range(key) {
                    continue;
                }
                if t.bloom.contains(&key) {
                    v.push((t.path.clone(), t.index.clone()));
                }
            }
            v
        };
        // Lock released — background compaction can proceed

        for (path, index) in &candidates {
            if let Some(v) = Self::search_one(path, index, key, &mut self.block_cache) {
                return Some(v);
            }
        }

        Some(Value::Tombstone)
    }

    pub fn search_one(
        path: &str,
        index: &crate::sstable::SSTableIndex,
        key: &str,
        cache: &mut BlockCache,
    ) -> Option<Value> {
        println!("checking SSTable: {}", path);

        let block = match find_block(index, key) {
            Some(b) => b,
            None => return None,
        };

        let cache_key = CacheKey {
            path: path.to_string(),
            offset: block.offset,
        };

        // Cache hit — no disk I/O
        if let Some(records) = cache.get(&cache_key) {
            println!("CACHE HIT: {:?}", cache_key);

            return binary_search_block(&records, key);
        }

        // Cache miss — read from disk (lock is NOT held at this point)
        println!("CACHE MISS: {:?}", cache_key);
        let records = match read_block(path, block.offset) {
            Ok(r) => r,
            Err(e) => {
                println!("BLOCK READ ERROR: {:?}", e);
                return None;
            }
        };

        cache.insert(cache_key, records.clone());

        return binary_search_block(&records, key);
    }

    pub fn maybe_flush(&mut self) -> Result<()> {
        if self.memtable.len() >= self.memtable_limit {
            let path = format!("sst_l0_{}.bin", next_sstable_id());
            self.flush_to_sstable(&path)?;
        }
        Ok(())
    }

    pub fn memtable_size(&self) -> usize {
        self.memtable.len()
    }

    pub fn scan(&self, start: &str, end: &str) -> Vec<(String, Value)> {
        let mut sources: Vec<Vec<(String, Value)>> = Vec::new();
        let mem_data: Vec<(String, Value)> = self
            .memtable
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        sources.push(mem_data);

        let sstables = self.sstables.lock().unwrap();
        for table in sstables.l0.iter().rev() {
            let data = read_sstable(&table.path).expect("Scan of L0 Failed!");
            sources.push(data);
        }
        for table in sstables.l1.iter().rev() {
            let data = read_sstable(&table.path).expect("Scan of L1 failed");
            sources.push(data);
        }
        for table in sstables.l2.iter().rev() {
            let data = read_sstable(&table.path).expect("Scan of L2 failed");
            sources.push(data);
        }

        let mut heap = BinaryHeap::new();
        let mut positions = vec![0usize; sources.len()];

        for (src_idx, source) in sources.iter().enumerate() {
            if let Some((k, v)) = source.first() {
                heap.push(ScanHeapItem {
                    key: k.clone(),
                    val: v.clone(),
                    source_idx: src_idx,
                });
            }
        }

        let mut merged: HashMap<String, Value> = HashMap::new();

        while let Some(item) = heap.pop() {
            if item.key.as_str() >= start && item.key.as_str() < end {
                merged.entry(item.key.clone()).or_insert(item.val.clone());
            }
            let src = item.source_idx;
            positions[src] += 1;
            if let Some((k, v)) = sources[src].get(positions[src]) {
                heap.push(ScanHeapItem {
                    key: k.clone(),
                    val: v.clone(),
                    source_idx: src,
                });
            }
        }

        let mut result: Vec<_> = merged
            .into_iter()
            .filter(|(_, v)| matches!(v, Value::Data(_)))
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    pub fn iter(&mut self) -> Result<UnifiedStorageIterator> {
        let mut iterators: Vec<Box<dyn StorageIterator>> = Vec::new();

        iterators.push(Box::new(MemtableIterator::new(&self.memtable)));

        let sstables = self.sstables.lock().unwrap();

        for table in sstables.l0.iter().rev() {
            iterators.push(Box::new(SSTableIterator::new(
                &table.path,
                table.index.clone(),
            )?));
        }

        for table in sstables.l1.iter().rev() {
            iterators.push(Box::new(SSTableIterator::new(
                &table.path,
                table.index.clone(),
            )?));
        }

        for table in sstables.l2.iter().rev() {
            iterators.push(Box::new(SSTableIterator::new(
                &table.path,
                table.index.clone(),
            )?));
        }

        // let merge = MergeIterator::new(iterators, false)?;

        UnifiedStorageIterator::new(iterators)
    }

    fn replay_wal(&mut self) -> Result<()> {

        let commands = self.storage.load()?;

        for command in commands {

            match command {

                Command::Set(key, value) => {
                    self.memtable.insert(
                        key,
                        Value::Data(value),
                    );
                }

                Command::Del(key) => {
                    self.memtable.insert(
                        key,
                        Value::Tombstone,
                    );
                }

                _ => {}
            }
        }

        Ok(())
    }
}