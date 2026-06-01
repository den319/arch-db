use std::{collections::{BTreeMap, HashMap}, fs::{self}};

use bloom::{ASMS, BloomFilter};

use crate::{engine::Value, error::Result, sstable::{BLOCK_SIZE, BlockMeta, SSTableIndex, read_sstable, search_sstable, write_sstable}};


pub struct SSTableManager {
    pub l0: Vec<SSTable>,
    pub l1: Vec<SSTable>,
    pub l2: Vec<SSTable>,

}

#[derive(Debug, Clone, Copy)]
pub enum Level {
    L0,
    L1,
    L2,
}

// #[derive(Debug)]

pub struct SSTable {
    pub path: String,
    pub index: SSTableIndex,
    pub bloom: BloomFilter,
    pub level: Level,
}

impl SSTableManager {
    pub fn new() -> Self {
        Self {
            l0: Vec::new(),
            l1: Vec::new(),
            l2: Vec::new(),

        }
    }

    pub fn add_table(&mut self, table: SSTable) {
        match table.level {
            Level::L0 => self.l0.push(table),
            Level::L1 => self.l1.push(table),
            Level::L2 => self.l2.push(table),
        }
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

        let mut offsets= BTreeMap::new();
        let mut blocks= Vec::new();

        let mut offset= 0u64;
        let mut current_block_size= 0usize;

        let size = data.len().max(8) as u32;
        let mut bloom= BloomFilter::with_rate(0.01, size);
        if data.is_empty() {
            self.add_table(SSTable {
                path: path.to_string(),
                index: SSTableIndex { offsets, blocks },
                bloom,
                level: Level::L0,
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
            level
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

        let path= format!("sst_l1_{}.bin", discover_sstables());

        let index= write_sstable(&path, &sorted)?;

        self.l1.push(SSTable { path, index, bloom, level: Level::L1 });

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

        let path= format!("sst_l1_{}.bin", discover_sstables());

        let index= write_sstable(&path, &sorted)?;

        let mut bloom= BloomFilter::with_rate(0.01, sorted.len().max(8) as u32);

        for (k,_) in &sorted {
            bloom.insert(k);
        }

        for table in &self.l0 {
            let _= fs::remove_file(&table.path);
        }

        self.l0.clear();

        self.l1.push(SSTable { path, index, bloom, level: Level::L1 });

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

        if !name.starts_with("sst_") && !name.ends_with(".bin") {
            continue;
        }

        let stem= name.trim_end_matches(".bin");

        if let Some(last)= stem.rsplit('_').next() {
            
            if let Ok(id)= last.parse::<usize>() {
                max_id= max_id.max(id);
            }
        }
    }        

    max_id + 1
}


#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

use crate::sstable::search_sstable;

    use super::*;

    #[test]
    fn test_add_table_to_correct_level() {
        let mut manager = SSTableManager::new();

        let bloom = BloomFilter::with_rate(0.01, 8);

        let table = SSTable {
            path: "test.bin".to_string(),
            index: SSTableIndex {
                offsets: BTreeMap::new(),
                blocks: Vec::new(),
            },
            bloom,
            level: Level::L1,
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

        let index1 = write_sstable("test_compact_l0_to_l1_test_l0_1.bin", &data1).unwrap();
        let index2 = write_sstable("test_compact_l0_to_l1_test_l0_2.bin", &data2).unwrap();

        let mut bloom1 = BloomFilter::with_rate(0.01, 8);
        bloom1.insert(&"a");
        bloom1.insert(&"b");

        let mut bloom2 = BloomFilter::with_rate(0.01, 8);
        bloom2.insert(&"c");
        bloom2.insert(&"d");

        manager.l0.push(SSTable {
            path: "test_compact_l0_to_l1_test_l0_1.bin".to_string(),
            index: index1,
            bloom: bloom1,
            level: Level::L0,
        });

        manager.l0.push(SSTable {
            path: "test_compact_l0_to_l1_test_l0_2.bin".to_string(),
            index: index2,
            bloom: bloom2,
            level: Level::L0,
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

        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let test_old_file = format!("test_old_{}.bin", id);

        let index1 = write_sstable(&test_old_file, &old_data).unwrap();
        let index2 = write_sstable("test_l0_compaction_keeps_latest_value_test_new.bin", &new_data).unwrap();

        let mut bloom1 = BloomFilter::with_rate(0.01, 8);
        bloom1.insert(&"user");

        let mut bloom2 = BloomFilter::with_rate(0.01, 8);
        bloom2.insert(&"user");

        manager.l0.push(SSTable {
            path: test_old_file.to_string(),
            index: index1,
            bloom: bloom1,
            level: Level::L0,
        });

        manager.l0.push(SSTable {
            path: "test_l0_compaction_keeps_latest_value_test_new.bin".to_string(),
            index: index2,
            bloom: bloom2,
            level: Level::L0,
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

        let index1 = write_sstable("test_tombstone_survives_compaction_data.bin", &old_data).unwrap();
        let index2 = write_sstable("test_tombstone_survives_compaction_delete.bin", &deleted_data).unwrap();

        let mut bloom1 = BloomFilter::with_rate(0.01, 8);
        bloom1.insert(&"user");

        let mut bloom2 = BloomFilter::with_rate(0.01, 8);
        bloom2.insert(&"user");

        manager.l0.push(SSTable {
            path: "test_tombstone_survives_compaction_data.bin".to_string(),
            index: index1,
            bloom: bloom1,
            level: Level::L0,
        });

        manager.l0.push(SSTable {
            path: "test_tombstone_survives_compaction_delete.bin".to_string(),
            index: index2,
            bloom: bloom2,
            level: Level::L0,
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

        write_sstable("test_load_from_file_into_l1_test.bin", &data).unwrap();

        manager.load_from_file(
            "test_load_from_file_into_l1_test.bin",
            Level::L1
        );

        assert_eq!(manager.l0.len(), 0);
        assert_eq!(manager.l1.len(), 1);
        assert_eq!(manager.l2.len(), 0);

        std::fs::remove_file("test_load_from_file_into_l1_test.bin").unwrap();
    }

}