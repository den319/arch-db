use std::{collections::{BTreeMap, HashMap}, fs::{self}, sync::atomic::{AtomicU64, Ordering}};

use bloom::{ASMS, BloomFilter};

use crate::{engine::Value, error::Result, sstable::{BLOCK_SIZE, BlockMeta, SSTableIndex, read_sstable, search_sstable, write_sstable}};


pub struct SSTableManager {
    pub l0: Vec<SSTable>,
    pub l1: Vec<SSTable>,
    pub l2: Vec<SSTable>,

    pub strategy: CompactionStrategy,
}

#[derive(Debug, Clone, Copy)]
pub enum Level {
    L0,
    L1,
    L2,
}

#[derive(Debug, Clone, Copy)]
pub enum CompactionStrategy {
    SizeTiered,
    Leveled,
}

// #[derive(Debug)]

pub struct SSTable {
    pub path: String,
    pub index: SSTableIndex,
    pub bloom: BloomFilter,
    pub level: Level,

    pub min_key: String,
    pub max_key: String,

    pub file_size: u64,
}


static SSTABLE_COUNTER: AtomicU64 = AtomicU64::new(0);

const L1_COMPACTION_THRESHOLD: usize = 4;

impl SSTableManager {
    pub fn new() -> Self {
        Self {
            l0: Vec::new(),
            l1: Vec::new(),
            l2: Vec::new(),
            strategy: CompactionStrategy::Leveled,
        }
    }

    pub fn maybe_compact(&mut self) -> Result<()> {
        match self.strategy {
            CompactionStrategy::Leveled => {
                if self.l0.len() >= 4 {
                    self.compact_l0_to_l1()?;
                }

                if self.l1.len() >= 4 {
                    self.compact_l1_to_l2()?;
                }
            }

            CompactionStrategy::SizeTiered => {

            }
        }

        Ok(())
    }

    pub fn add_table(&mut self, table: SSTable) {
        match table.level {
            Level::L0 => self.l0.push(table),
            Level::L1 => self.l1.push(table),
            Level::L2 => self.l2.push(table),
        }
    }

    pub fn load_table(&self, table: &SSTable) -> Result<BTreeMap<String, Value>> {
        let data= read_sstable(&table.path)?;

        let mut result= BTreeMap::new();

        for (k,v) in data {
            result.insert(k, v);
        }

        Ok(result)
    }

    pub fn get(&self, key:&str) -> Result<Option<Value>> {
        for table in self.l0.iter().rev() {
            if !table.bloom.contains(&key) {
                continue;
            }

            if let Some((_, value)) =
                search_sstable(&table.path, &table.index, key)?
            {
                return Ok(Some(value));
            }
        }

        // L1 newest -> oldest
        for table in self.l1.iter().rev() {
            if !table.bloom.contains(&key) {
                continue;
            }

            if let Some((_, value)) =
                search_sstable(&table.path, &table.index, key)?
            {
                return Ok(Some(value));
            }
        }

        // L2 newest -> oldest
        for table in self.l2.iter().rev() {
            if !table.bloom.contains(&key) {
                continue;
            }

            if let Some((_, value)) =
                search_sstable(&table.path, &table.index, key)?
            {
                return Ok(Some(value));
            }
        }

        Ok(None)
    }

    pub fn load_from_file(&mut self, path: &str, level: Level) {
        // println!("{}", path);
        let data= read_sstable(path).expect("Failed to read sstable!");

        let min_key= data.first().map(|(k, _)|k.clone()).unwrap_or_default();

        let max_key= data.last().map(|(k, _)|k.clone()).unwrap_or_default();


        let mut offsets= BTreeMap::new();
        let mut blocks= Vec::new();

        let mut offset= 0u64;
        let mut current_block_size= 0usize;

        let size = data.len().max(8) as u32;
        let mut bloom= BloomFilter::with_rate(0.01, size);
        
        let file_size = fs::metadata(path)
            .expect("failed to read metadata")
            .len();

        if data.is_empty() {
            self.add_table(SSTable {
                path: path.to_string(),
                index: SSTableIndex { offsets, blocks },
                bloom,
                level: Level::L0,
                min_key,
                max_key,
                file_size,
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
                    offset,
                    record_offset: BTreeMap::new(),
                });
            }

            if let Some(last_block) = blocks.last_mut() {
                last_block.record_offset.insert(
                    key.clone(),
                    offset
                );
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
            level,
            min_key,
            max_key,
            file_size,
        };

        // println!("{:?}", table);

        self.add_table(table);
    }

    fn all_tables(&self) -> impl Iterator<Item = &SSTable> {
        self.l0
            .iter()
            .chain(self.l1.iter())
            .chain(self.l2.iter())
    }

    pub fn compact(&mut self) -> Result<()> {
        let mut merged= HashMap::new();


        for table in self.all_tables() {
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

        let min_key= sorted.first().map(|(k, _)|k.clone()).unwrap();

        let max_key= sorted.last().map(|(k, _)|k.clone()).unwrap();


        for table in self.all_tables() {
            let _= fs::remove_file(&table.path)?;
        }
        self.l0.clear();
        self.l1.clear();
        self.l2.clear();

        
        if sorted.is_empty() {
            return Ok(());
        }

        let mut bloom = BloomFilter::with_rate(0.01, sorted.len() as u32);

        for (key, _) in &sorted {
            bloom.insert(key);
        }

        let path= format!("sst_l1_{}.bin", next_sstable_id());

        let index= write_sstable(&path, &sorted)?;

        let file_size = fs::metadata(&path)?.len();

        self.l1.push(SSTable { path, index, bloom, level: Level::L1, min_key, max_key, file_size });

        Ok(())
    }

    pub fn compact_l0_to_l1(&mut self) -> Result<()> {
        if self.l0.is_empty() {
            return Ok(());
        }

        let mut merged= BTreeMap::<String, Value>::new();

        for table in &self.l0 {
            let data= read_sstable(&table.path)?;

            for (k,v) in data {
                match v {
                    Value::Data(val) => {
                        merged.insert(k, Value::Data(val));
                    }
                    Value::Tombstone => {
                        merged.insert(k, Value::Tombstone);
                    }
                }
            }
        }

        let sorted: Vec<_>= merged.into_iter().collect();

        let path= format!("sst_l1_{}.bin", next_sstable_id());

        let index= write_sstable(&path, &sorted)?;

        let mut bloom= BloomFilter::with_rate(0.01, sorted.len().max(8) as u32);

        let min_key= sorted.first().map(|(k, _)|k.clone()).unwrap();

        let max_key= sorted.last().map(|(k, _)|k.clone()).unwrap();


        for (k,_) in &sorted {
            bloom.insert(k);
        }

        for table in &self.l0 {
            let _= fs::remove_file(&table.path);
        }

        self.l0.clear();

        let file_size= fs::metadata(&path)?.len();

        self.l1.push(SSTable { path, index, bloom, level: Level::L1, min_key, max_key, file_size });

        if self.l1.len() >= L1_COMPACTION_THRESHOLD {
            let _= self.compact_l1_to_l2();
        }

        Ok(())
    }

    pub fn compact_l1_to_l2(&mut self) -> Result<()> {
        if self.l1.is_empty() {
            return Ok(());
        }

        let l1_min = self.l1[0].min_key.clone();
        let l1_max = self.l1[0].max_key.clone();

        let overlapping_indices:Vec<usize> = self.l2.iter().enumerate()
            .filter(|(_,table)| {
                table.overlaps(&l1_min, &l1_max)
            })
            .map(|(idx, _)| idx)
            .collect();

        println!("Found {} overlapping tables",overlapping_indices.len());

        let mut merged= BTreeMap::new();

        for idx in &overlapping_indices {
            let table= &self.l2[*idx];

            let data= self.load_table(table)?;

            for (k,v) in data {
                merged.insert(k,v);
            }
        }

        let l1_data= self.load_table(&self.l1[0])?;

        for (k,v) in l1_data {
            merged.insert(k, v);
        }

        let sorted: Vec<(String, Value)>= merged.into_iter().collect();

        let mut bloom= BloomFilter::with_rate(0.01, sorted.len().max(8) as u32);

        for (k,_) in &sorted {
            bloom.insert(k);
        }

        let min_key= sorted.first().map(|(k,_)| k.clone()).unwrap();

        let max_key= sorted.last().map(|(k,_)| k.clone()).unwrap();

        let path= format!("sst_l2_{}.bin", next_sstable_id());

        let index= write_sstable(&path, &sorted)?;

        let file_size= fs::metadata(&path)?.len();


        let new_table= SSTable {
            path: path.clone(),
            index, 
            bloom,
            level: Level::L2,
            min_key,
            max_key,
            file_size
        };

        for idx in overlapping_indices.iter().rev() {
            let table= self.l2.remove(*idx);

            fs::remove_file(&table.path)?;
        }

        fs::remove_file(&self.l1[0].path)?;

        self.l1.remove(0);

        self.l2.push(new_table);

        Ok(())
    }
}


pub fn init_sstable_counter() {
    let mut max_id = 0u64;

    let entries = fs::read_dir(".")
        .expect("Failed to read directory");

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if !name.starts_with("sst_") || !name.ends_with(".bin") {
            continue;
        }

        let stem = name.trim_end_matches(".bin");

        if let Some(id_str) = stem.rsplit('_').next() {
            if let Ok(id) = id_str.parse::<u64>() {
                max_id = max_id.max(id);
            }
        }
    }

    SSTABLE_COUNTER.store(max_id + 1, Ordering::SeqCst);
}

pub fn next_sstable_id() -> u64 {
    SSTABLE_COUNTER.fetch_add(1, Ordering::SeqCst)
}


#[cfg(test)]
mod tests {
    use crate::{helper::unique_file, sstable::search_sstable};

    use super::*;

    #[test]
    fn test_add_table_to_correct_level() {
        let mut manager = SSTableManager::new();

        let bloom = BloomFilter::with_rate(0.01, 8);

        let table = SSTable {
            path: "test_add_table_to_correct_level.bin".to_string(),
            index: SSTableIndex {
                offsets: BTreeMap::new(),
                blocks: Vec::new(),
            },
            bloom,
            level: Level::L1,
            min_key: "a".to_string(),
            max_key: "z".to_string(),
            file_size:0,
        };

        manager.add_table(table);

        assert_eq!(manager.l0.len(), 0);
        assert_eq!(manager.l1.len(), 1);
        assert_eq!(manager.l2.len(), 0);
    }

    #[test]
    fn test_compact_l0_to_l1() {
        let mut manager = SSTableManager::new();

        let data1 = vec![
            ("a".to_string(), Value::Data("1".to_string())),
            ("b".to_string(), Value::Data("2".to_string())),
        ];

        let data2 = vec![
            ("c".to_string(), Value::Data("3".to_string())),
            ("d".to_string(), Value::Data("4".to_string())),
        ];

        let file1= unique_file("test_compact_l0_to_l1_test_l0_1", "bin");
        let file2= unique_file("test_compact_l0_to_l1_test_l0_2", "bin");


        let index1 = write_sstable(&file1, &data1).unwrap();
        let index2 = write_sstable(&file2, &data2).unwrap();

        let mut bloom1 = BloomFilter::with_rate(0.01, 8);
        bloom1.insert(&"a");
        bloom1.insert(&"b");

        let mut bloom2 = BloomFilter::with_rate(0.01, 8);
        bloom2.insert(&"c");
        bloom2.insert(&"d");

        let file1_size = fs::metadata(&file1).expect("Failed to read metadata of file-1").len();
        let file2_size = fs::metadata(&file2).expect("Failed to read metadata of file-2").len();


        manager.l0.push(SSTable {
            path: file1,
            index: index1,
            bloom: bloom1,
            level: Level::L0,
            min_key: "a".to_string(),
            max_key: "b".to_string(),
            file_size: file1_size,
            
        });

        manager.l0.push(SSTable {
            path: file2,
            index: index2,
            bloom: bloom2,
            level: Level::L0,
            min_key: "c".to_string(),
            max_key: "d".to_string(),
            file_size: file2_size,
        });

        manager.compact_l0_to_l1().unwrap();

        assert_eq!(manager.l0.len(), 0);
        assert_eq!(manager.l1.len(), 1);

        std::fs::remove_file(&manager.l1[0].path).unwrap();
    }

    #[test]
    fn test_l0_compaction_keeps_latest_value() {
        let mut manager = SSTableManager::new();

        let old_data = vec![
            ("user".to_string(), Value::Data("old".to_string()))
        ];

        let new_data = vec![
            ("user".to_string(), Value::Data("new".to_string()))
        ];

        let test_old_file= unique_file("test_l0_compaction_keeps_latest_value_test_old", "bin");
        let test_new_file= unique_file("test_l0_compaction_keeps_latest_value_test_new", "bin");

        let index1 = write_sstable(&test_old_file, &old_data).unwrap();
        let index2 = write_sstable(&test_new_file, &new_data).unwrap();

        let mut bloom1 = BloomFilter::with_rate(0.01, 8);
        bloom1.insert(&"user");

        let mut bloom2 = BloomFilter::with_rate(0.01, 8);
        bloom2.insert(&"user");

        let file_old_size = fs::metadata(&test_old_file).expect("Failed to read metadata of old file").len();
        let file_new_size = fs::metadata(&test_new_file).expect("Failed to read metadata of new file").len();


        manager.l0.push(SSTable {
            path: test_old_file,
            index: index1,
            bloom: bloom1,
            level: Level::L0,
            min_key: "user".to_string(),
            max_key: "user".to_string(),
            file_size: file_old_size,
        });

        manager.l0.push(SSTable {
            path: test_new_file,
            index: index2,
            bloom: bloom2,
            level: Level::L0,
            min_key: "user".to_string(),
            max_key: "user".to_string(),
            file_size: file_new_size,
        });

        manager.compact_l0_to_l1().unwrap();

        let table = &manager.l1[0];

        let result = search_sstable(
            &table.path,
            &table.index,
            "user"
        ).unwrap();

        match result {
            Some((_, Value::Data(v))) => assert_eq!(v, "new"),
            _ => panic!("wrong value"),
        }

        let _= std::fs::remove_file(&table.path).unwrap();
    }

    #[test]
    fn test_tombstone_survives_compaction() {
        let mut manager = SSTableManager::new();

        let old_data = vec![
            ("user".to_string(), Value::Data("john".to_string()))
        ];

        let deleted_data = vec![
            ("user".to_string(), Value::Tombstone)
        ];

        let file1= unique_file("test_tombstone_survives_compaction_data", "bin");
        let file2= unique_file("test_tombstone_survives_compaction_delete", "bin");
        

        let index1 = write_sstable(&file1, &old_data).unwrap();
        let index2 = write_sstable(&file2, &deleted_data).unwrap();

        let mut bloom1 = BloomFilter::with_rate(0.01, 8);
        bloom1.insert(&"user");

        let mut bloom2 = BloomFilter::with_rate(0.01, 8);
        bloom2.insert(&"user");

        let file1_size = fs::metadata(&file1).expect("Failed to read metadata of file-1").len();
        let file2_size = fs::metadata(&file2).expect("Failed to read metadata of file-2").len();


        manager.l0.push(SSTable {
            path: file1,
            index: index1,
            bloom: bloom1,
            level: Level::L0,
            min_key: "user".to_string(),
            max_key: "user".to_string(),
            file_size: file1_size,

        });

        manager.l0.push(SSTable {
            path: file2,
            index: index2,
            bloom: bloom2,
            level: Level::L0,
            min_key: "user".to_string(),
            max_key: "user".to_string(),
            file_size: file2_size,
        });

        manager.compact_l0_to_l1().unwrap();

        let table = &manager.l1[0];

        let result = search_sstable(
            &table.path,
            &table.index,
            "user"
        ).unwrap();

        println!("RESULT = {:?}", result);

        match result {
            Some((_, Value::Tombstone)) => {}
            _ => panic!("expected tombstone"),
        }

        std::fs::remove_file(&table.path).unwrap();
    }

    #[test]
    fn test_load_from_file_into_l1() {
        let mut manager = SSTableManager::new();

        let data = vec![
            ("a".to_string(), Value::Data("1".to_string()))
        ];

        let file1= unique_file("test_load_from_file_into_l1_test", "bin");
        

        write_sstable(&file1, &data).unwrap();

        manager.load_from_file(
            &file1,
            Level::L1
        );

        assert_eq!(manager.l0.len(), 0);
        assert_eq!(manager.l1.len(), 1);
        assert_eq!(manager.l2.len(), 0);

        std::fs::remove_file(&file1).unwrap();
    }

    #[test]
    fn test_sstable_range_metadata() {
        let data = vec![
            ("apple".to_string(), Value::Data("1".to_string())),
            ("banana".to_string(), Value::Data("2".to_string())),
            ("orange".to_string(), Value::Data("3".to_string())),
        ];

        let file = unique_file("test_sstable_range_metadata", "bin");

        let index = write_sstable(&file, &data).unwrap();

        let mut bloom = BloomFilter::with_rate(0.01, 8);

        for (k, _) in &data {
            bloom.insert(k);
        }

        let file_size = fs::metadata(&file).expect("Failed to read metadata of file").len();


        let table = SSTable {
            path: file.clone(),
            index,
            bloom,
            level: Level::L0,
            min_key: "apple".to_string(),
            max_key: "orange".to_string(),
            file_size
        };

        assert_eq!(table.min_key, "apple");
        assert_eq!(table.max_key, "orange");

        std::fs::remove_file(file).unwrap();
    }

    #[test]
    fn test_sstable_range_contains_key() {
        let file= unique_file("test_sstable_range_contains_key", "bin");

        let table = SSTable {
            path: file,
            index: SSTableIndex {
                offsets: BTreeMap::new(),
                blocks: Vec::new(),
            },
            bloom: BloomFilter::with_rate(0.01, 8),
            level: Level::L1,
            min_key: "apple".to_string(),
            max_key: "orange".to_string(),
            file_size: 0,
        };

        assert!(table.contains_key_range("apple"));
        assert!(table.contains_key_range("banana"));
        assert!(table.contains_key_range("orange"));

        assert!(!table.contains_key_range("aardvark"));
        assert!(!table.contains_key_range("zebra"));
    }

    #[test]
    fn test_overlap_detection() {
        let file= unique_file("test_overlap_detection", "bin");
        let table = SSTable {
            path: file,
            index: SSTableIndex {
                offsets: BTreeMap::new(),
                blocks: Vec::new(),
            },
            bloom: BloomFilter::with_rate(0.01, 8),
            level: Level::L1,
            min_key: "g".to_string(),
            max_key: "m".to_string(),
            file_size: 0,
        };

        assert!(table.contains_key_range("g"));
        assert!(table.contains_key_range("k"));
        assert!(table.contains_key_range("m"));

        assert!(!table.contains_key_range("a"));
        assert!(!table.contains_key_range("z"));
    }

    #[test]
    fn test_compact_l1_to_l2_basic() {
        let mut manager = SSTableManager::new();

        let l1_data = vec![
            ("a".to_string(), Value::Data("1".to_string())),
            ("b".to_string(), Value::Data("2".to_string())),
        ];

        let l1_file = unique_file("test_compact_l1_to_l2_basic", "bin");

        let l1_index = write_sstable(&l1_file, &l1_data).unwrap();

        let mut l1_bloom = BloomFilter::with_rate(0.01, 8);

        l1_bloom.insert(&"a");
        l1_bloom.insert(&"b");

        let file_size = fs::metadata(&l1_file).expect("Failed to read metadata of file").len();


        manager.l1.push(SSTable {
            path: l1_file.clone(),
            index: l1_index,
            bloom: l1_bloom,
            level: Level::L1,
            min_key: "a".to_string(),
            max_key: "b".to_string(),
            file_size,
        });

        manager.compact_l1_to_l2().unwrap();

        assert_eq!(manager.l1.len(), 0);
        assert_eq!(manager.l2.len(), 1);

        let table = &manager.l2[0];

        let result = search_sstable(
            &table.path,
            &table.index,
            "a"
        ).unwrap();

        match result {
            Some((_, Value::Data(v))) => assert_eq!(v, "1"),
            _ => panic!("wrong value"),
        }

        fs::remove_file(&table.path).unwrap();
    }

    #[test]
    fn test_l1_overwrites_l2_during_compaction() {
        let mut manager = SSTableManager::new();

        let l2_data = vec![
            ("user".to_string(), Value::Data("old".to_string()))
        ];

        let l1_data = vec![
            ("user".to_string(), Value::Data("new".to_string()))
        ];

        let l2_file = unique_file("test_l1_overwrites_l2_during_compaction_old", "bin");
        let l1_file = unique_file("test_l1_overwrites_l2_during_compaction_new", "bin");

        let l2_index = write_sstable(&l2_file, &l2_data).unwrap();
        let l1_index = write_sstable(&l1_file, &l1_data).unwrap();

        let mut l2_bloom = BloomFilter::with_rate(0.01, 8);
        l2_bloom.insert(&"user");

        let mut l1_bloom = BloomFilter::with_rate(0.01, 8);
        l1_bloom.insert(&"user");

        let file1_size = fs::metadata(&l1_file).expect("Failed to read metadata of file-1").len();
        let file2_size = fs::metadata(&l2_file).expect("Failed to read metadata of file-2").len();


        manager.l2.push(SSTable {
            path: l2_file,
            index: l2_index,
            bloom: l2_bloom,
            level: Level::L2,
            min_key: "user".to_string(),
            max_key: "user".to_string(),
            file_size: file2_size,
        });

        manager.l1.push(SSTable {
            path: l1_file,
            index: l1_index,
            bloom: l1_bloom,
            level: Level::L1,
            min_key: "user".to_string(),
            max_key: "user".to_string(),
            file_size: file1_size,
        });

        manager.compact_l1_to_l2().unwrap();

        assert_eq!(manager.l1.len(), 0);
        assert_eq!(manager.l2.len(), 1);

        let table = &manager.l2[0];

        let result = search_sstable(
            &table.path,
            &table.index,
            "user"
        ).unwrap();

        match result {
            Some((_, Value::Data(v))) => {
                assert_eq!(v, "new");
            }
            _ => panic!("expected latest value"),
        }

        fs::remove_file(&table.path).unwrap();
    }

    #[test]
    fn test_tombstone_overwrites_l2_data() {
        let mut manager = SSTableManager::new();

        let l2_data = vec![
            ("user".to_string(), Value::Data("john".to_string()))
        ];

        let l1_data = vec![
            ("user".to_string(), Value::Tombstone)
        ];

        let l2_file = unique_file("test_tombstone_overwrites_l2_data_old", "bin");
        let l1_file = unique_file("test_tombstone_overwrites_l2_data_new", "bin");

        let l2_index = write_sstable(&l2_file, &l2_data).unwrap();
        let l1_index = write_sstable(&l1_file, &l1_data).unwrap();

        let mut l2_bloom = BloomFilter::with_rate(0.01, 8);
        l2_bloom.insert(&"user");

        let mut l1_bloom = BloomFilter::with_rate(0.01, 8);
        l1_bloom.insert(&"user");

        let file1_size = fs::metadata(&l1_file).expect("Failed to read metadata of file-1").len();
        let file2_size = fs::metadata(&l2_file).expect("Failed to read metadata of file-2").len();


        manager.l2.push(SSTable {
            path: l2_file,
            index: l2_index,
            bloom: l2_bloom,
            level: Level::L2,
            min_key: "user".to_string(),
            max_key: "user".to_string(),
            file_size: file2_size,
        });

        manager.l1.push(SSTable {
            path: l1_file,
            index: l1_index,
            bloom: l1_bloom,
            level: Level::L1,
            min_key: "user".to_string(),
            max_key: "user".to_string(),
            file_size: file1_size,
        });

        manager.compact_l1_to_l2().unwrap();

        let table = &manager.l2[0];

        let result = search_sstable(
            &table.path,
            &table.index,
            "user"
        ).unwrap();

        match result {
            Some((_, Value::Tombstone)) => {}
            _ => panic!("expected tombstone"),
        }

        fs::remove_file(&table.path).unwrap();
    }

    #[test]
    fn test_non_overlapping_l2_table_survives() {
        let mut manager = SSTableManager::new();

        let l2_old_data = vec![
            ("x".to_string(), Value::Data("100".to_string()))
        ];

        let l1_data = vec![
            ("a".to_string(), Value::Data("1".to_string()))
        ];

        let l2_file = unique_file("test_l2_non_overlap", "bin");
        let l1_file = unique_file("test_l1_non_overlap", "bin");

        let l2_index = write_sstable(&l2_file, &l2_old_data).unwrap();
        let l1_index = write_sstable(&l1_file, &l1_data).unwrap();

        let mut l2_bloom = BloomFilter::with_rate(0.01, 8);
        l2_bloom.insert(&"x");

        let mut l1_bloom = BloomFilter::with_rate(0.01, 8);
        l1_bloom.insert(&"a");

        let file1_size = fs::metadata(&l1_file).expect("Failed to read metadata of file-1").len();
        let file2_size = fs::metadata(&l2_file).expect("Failed to read metadata of file-2").len();


        manager.l2.push(SSTable {
            path: l2_file.clone(),
            index: l2_index,
            bloom: l2_bloom,
            level: Level::L2,
            min_key: "x".to_string(),
            max_key: "x".to_string(),
            file_size: file2_size,
        });

        manager.l1.push(SSTable {
            path: l1_file,
            index: l1_index,
            bloom: l1_bloom,
            level: Level::L1,
            min_key: "a".to_string(),
            max_key: "a".to_string(),
            file_size: file1_size,
        });

        manager.compact_l1_to_l2().unwrap();

        assert_eq!(manager.l2.len(), 2);

        assert!(
            manager.l2.iter().any(|t| t.min_key == "x")
        );

        for table in &manager.l2 {
            let _ = fs::remove_file(&table.path);
        }
    }

    #[test]
    fn test_maybe_compact_triggers_l0_to_l1() {
        let mut manager = SSTableManager::new();

        for i in 0..4 {

            let data = vec![
                (
                    format!("k{}", i),
                    Value::Data(format!("v{}", i))
                )
            ];

            let file = unique_file("auto_compact", "bin");

            let index = write_sstable(&file, &data).unwrap();

            let mut bloom =
                BloomFilter::with_rate(0.01, 8);

            bloom.insert(&format!("k{}", i));

            manager.l0.push(SSTable {
                path: file.clone(),
                index,
                bloom,
                level: Level::L0,
                min_key: format!("k{}", i),
                max_key: format!("k{}", i),
                file_size: fs::metadata(&file).unwrap().len(),
            });
        }

        manager.maybe_compact().unwrap();

        assert_eq!(manager.l0.len(), 0);
        assert_eq!(manager.l1.len(), 1);

        for table in &manager.l1 {
            let _ = fs::remove_file(&table.path);
        }
    }

    #[test]
    fn test_sstable_file_size() {
        let data = vec![
            ("a".to_string(), Value::Data("1".to_string())),
            ("b".to_string(), Value::Data("2".to_string())),
        ];

        let file = unique_file("test_sstable_file_size", "bin");

        write_sstable(&file, &data).unwrap();

        let mut manager = SSTableManager::new();

        manager.load_from_file(&file, Level::L1);

        assert!(manager.l1[0].file_size > 0);

        fs::remove_file(file).unwrap();
    }





}



