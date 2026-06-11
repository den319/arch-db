use std::{cmp::Ordering, collections::{BTreeMap, BinaryHeap, HashMap}, fs, sync::{Arc, Mutex}};

use bloom::{ASMS, BloomFilter};

use crate::{command::Command, error::Result, sstable::{read_sstable, search_sstable, write_sstable}, sstable_manager::{Level, SSTable, SSTableManager, next_sstable_id}};


#[derive(Clone, Debug)]
pub enum Value {
    Data(String),
    Tombstone,
}

pub struct Engine {
    pub(crate) memtable: BTreeMap<String, Value>,
    pub(crate) sstables: Arc<Mutex<SSTableManager>>,
    pub(crate) memtable_limit: usize,
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
        Self {
            memtable: BTreeMap::new(),
            sstables: Arc::new(Mutex::new(SSTableManager::new())),
            memtable_limit: 1000,
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
                
                match self.sstables.compact() {
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

        self.sstables.add_table(
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

        let sstables = self.sstables.lock().unwrap();

        if sstables.l0.len() >= 4 {
            sstables.size_tiered_compact_l0()?;
        }

        sstables.maybe_compact()?;
        
        self.memtable.clear();

        Ok(())
    }

    pub fn get_key(&self, key:&str) -> Option<Value> {

        println!("GET KEY: {:?}", key);
        if let Some(val)= self.memtable.get(key) {
            // println!("{:?}", val);
            return Some(val.clone());
        }
        let sstables = self.sstables.lock().unwrap();


        if let Some(v) = Self::search_level(&sstables.l0, key) {
            return Some(v);
        }

        if let Some(v) = Self::search_level(&sstables.l1, key) {
            return Some(v);
        }   

        if let Some(v) = Self::search_level(&sstables.l2, key) {
            return Some(v);
        }

        Some(Value::Tombstone)
    }

    pub fn search_level(level: &[SSTable], key:&str) -> Option<Value> {
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

            match search_sstable(&table.path, &table.index, key) {
                Ok(Some((_, val))) => {
                    if let Value::Tombstone = val {
                        return Some(Value::Tombstone);
                    }
                    println!("Found in SSTable: {:?}", val);
                    return Some(val);
                }
                Ok(None) => {
                    println!("Not found in this block");
                }
                Err(e) => {
                    println!("SSTABLE READ ERROR: {:?}", e);
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

        for table in &self.sstables.l1 {
            let data = read_sstable(&table.path).expect("Scan of L1 failed");
            sources.push(data);
        }

        for table in &self.sstables.l2 {
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

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use crate::{command::Command, engine::{Engine, Value}, helper::unique_file, sstable_manager::init_sstable_counter};

    /// Initialize the global SSTABLE_COUNTER exactly once across all parallel tests,
    /// ensuring no test creates files that collide with each other or with user data.
    fn ensure_counter_initialized() {
        static INIT: OnceLock<()> = OnceLock::new();
        INIT.get_or_init(|| {
            init_sstable_counter();
        });
    }

    #[test]
    fn test_auto_flush_when_memtable_limit_reached() {
        ensure_counter_initialized();

        let mut engine = Engine::new();

        // Small threshold so flush happens quickly
        engine.memtable_limit = 2;

        engine.execute(
            Command::Set(
                "a".to_string(),
                "1".to_string(),
            )
        );

        let sstables = engine.sstables.lock().unwrap();

        // Still in memtable
        assert_eq!(engine.memtable.len(), 1);
        assert_eq!(sstables.l0.len(), 0);

        engine.execute(
            Command::Set(
                "b".to_string(),
                "2".to_string(),
            )
        );

        let sstables1 = engine.sstables.lock().unwrap();

        // Auto flush should have happened
        assert_eq!(engine.memtable.len(), 0);
        assert_eq!(sstables1.l0.len(), 1);

        // Data must still be readable
        match engine.get_key("a") {
            Some(Value::Data(v)) => assert_eq!(v, "1"),
            _ => panic!("expected value 1"),
        }

        match engine.get_key("b") {
            Some(Value::Data(v)) => assert_eq!(v, "2"),
            _ => panic!("expected value 2"),
        }

        // Cleanup generated SSTable
        std::fs::remove_file(
            &sstables1.l0[0].path
        )
        .unwrap();
    }

    #[test]
    fn test_auto_flush_preserves_tombstones() {
        ensure_counter_initialized();

        let mut engine = Engine::new();

        engine.memtable_limit = 2;

        engine.execute(
            Command::Set(
                "user".to_string(),
                "john".to_string(),
            )
        );

        engine.execute(
            Command::Del(
                "user".to_string()
            )
        );

        engine.execute(Command::Set(
            "another".to_string(),
            "x".to_string(),
        ));

        let sstables = engine.sstables.lock().unwrap();

        // Flush should have happened
        // assert_eq!(engine.memtable.len(), 0);
        assert_eq!(sstables.l0.len(), 1);

        match engine.get_key("user") {
            Some(Value::Tombstone) => {}
            _ => panic!("expected tombstone"),
        }

        std::fs::remove_file(
            &sstables.l0[0].path
        )
        .unwrap();
    }

    #[test]
    fn test_auto_l0_compaction_trigger() {
        ensure_counter_initialized();

        let mut engine = Engine::new();

        for i in 0..12 {
            engine.execute(
                Command::Set(
                    format!("key{}", i),
                    format!("value{}", i)
                )
            );

            // Use a unique file for each flush; size-tiered compaction needs
            // at least 3 files, and reusing the same file would get it deleted
            let file = unique_file("test_auto_l0_compaction_trigger", "bin");
            engine.flush_to_sstable(&file).unwrap();
        }

        let sstables = engine.sstables.lock().unwrap();

        assert!(sstables.l0.len() < 4);
        assert!(!engine.sstables.l1.is_empty());

        // Cleanup all files created by flush_to_sstable, including compaction artifacts
        let all_paths: Vec<String> = sstables.l0.iter()
            .chain(engine.sstables.l1.iter())
            .chain(engine.sstables.l2.iter())
            .map(|t| t.path.clone())
            .collect();
        
        for path in all_paths {
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn test_shared_sstable_manager_access() {
        ensure_counter_initialized();

        let engine = Engine::new();

        {
            let sstables =
                engine.sstables.lock().unwrap();

            assert_eq!(sstables.l0.len(), 0);
            assert_eq!(sstables.l1.len(), 0);
            assert_eq!(sstables.l2.len(), 0);
        }
    }


    #[test]
fn test_flush_with_shared_sstable_manager() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    engine.memtable_limit = 2;

    engine.execute(
        Command::Set(
            "a".to_string(),
            "1".to_string(),
        )
    );

    engine.execute(
        Command::Set(
            "b".to_string(),
            "2".to_string(),
        )
    );

    {
        let sstables =
            engine.sstables.lock().unwrap();

        assert_eq!(sstables.l0.len(), 1);
    }

    match engine.get_key("a") {
        Some(Value::Data(v)) => {
            assert_eq!(v, "1");
        }
        _ => panic!("expected value"),
    }

    let paths: Vec<String> = {
        let sstables =
            engine.sstables.lock().unwrap();

        sstables
            .l0
            .iter()
            .map(|t| t.path.clone())
            .collect()
    };

    for path in paths {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn test_arc_shares_same_sstable_manager() {
    ensure_counter_initialized();

    let engine = Engine::new();

    let shared1 = engine.sstables.clone();
    let shared2 = engine.sstables.clone();

    {
        let mut s1 =
            shared1.lock().unwrap();

        s1.l0.push(
            SSTable {
                path: "dummy.bin".to_string(),

                index: crate::sstable::SSTableIndex {
                    offsets: Default::default(),
                    blocks: vec![],
                },

                bloom: bloom::BloomFilter::with_rate(0.01, 8),

                level: crate::sstable_manager::Level::L0,

                min_key: "a".to_string(),
                max_key: "z".to_string(),

                file_size: 0,
            }
        );
    }

    {
        let s2 =
            shared2.lock().unwrap();

        assert_eq!(s2.l0.len(), 1);
    }
}

#[test]
fn test_multiple_lock_scopes() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    engine.execute(
        Command::Set(
            "name".to_string(),
            "dharmik".to_string(),
        )
    );

    {
        let sstables =
            engine.sstables.lock().unwrap();

        let _ = sstables.l0.len();
    }

    {
        let sstables =
            engine.sstables.lock().unwrap();

        let _ = sstables.l1.len();
    }

    match engine.get_key("name") {
        Some(Value::Data(v)) => {
            assert_eq!(v, "dharmik");
        }
        _ => panic!("expected value"),
    }
}

#[test]
fn test_compaction_with_shared_manager() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    for i in 0..12 {
        engine.execute(
            Command::Set(
                format!("key{}", i),
                format!("value{}", i),
            )
        );

        let file =
            unique_file(
                "shared_compaction",
                "bin"
            );

        engine.flush_to_sstable(&file)
            .unwrap();
    }

    {
        let sstables =
            engine.sstables.lock().unwrap();

        assert!(sstables.l0.len() < 4);

        assert!(!sstables.l1.is_empty());
    }

    let paths: Vec<String> = {
        let sstables =
            engine.sstables.lock().unwrap();

        sstables
            .l0
            .iter()
            .chain(sstables.l1.iter())
            .chain(sstables.l2.iter())
            .map(|t| t.path.clone())
            .collect()
    };

    for path in paths {
        let _ = std::fs::remove_file(path);
    }
}
}
