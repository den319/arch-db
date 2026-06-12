use std::{collections::{BTreeMap, HashMap}, fs::{self}, sync::atomic::{AtomicU64, Ordering}};

use bloom::{ASMS, BloomFilter};

use crate::{engine::Value, error::Result, sstable::{BLOCK_SIZE, BlockMeta, SSTableIndex, read_sstable, search_sstable, write_sstable}};


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


pub struct SSTableManager {
    pub l0: Vec<SSTable>,
    pub l1: Vec<SSTable>,
    pub l2: Vec<SSTable>,

    pub strategy: CompactionStrategy,
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
const SIZE_TIERED_MIN_TABLES: usize = 3;
const SIZE_TIERED_SIZE_RATIO: f64 = 1.5;

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
                    self.size_tiered_compact_l0()?;
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

    pub fn find_size_tiered_candidates(&self) -> Vec<usize> {
        if self.l0.len() < SIZE_TIERED_MIN_TABLES {
            return vec![];
        }

        let mut indexed: Vec<(usize, u64)> = self.l0
            .iter()
            .enumerate()
            .map(|(i, t)| (i, t.file_size))
            .collect();

        indexed.sort_by_key(|(_, size)| *size);

        for window in indexed.windows(SIZE_TIERED_MIN_TABLES) {
            let min = window.first().unwrap().1 as f64;
            let max = window.last().unwrap().1 as f64;

            if max / min <= SIZE_TIERED_SIZE_RATIO {
                return window.iter().map(|(idx, _)| *idx).collect();
            }
        }

        vec![]
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

    pub fn size_tiered_compact_l0(&mut self) -> Result<()> {
        let candidates = self.find_size_tiered_candidates();

        if candidates.is_empty() {
            return Ok(());
        }

        let mut merged = BTreeMap::<String, Value>::new();

        // Iterate candidates in ascending L0 index order (oldest first), so that
        // newer files (higher index) overwrite older ones via BTreeMap::insert.
        let mut indexed_candidates = candidates.clone();
        indexed_candidates.sort();
        for idx in &indexed_candidates {
            let table = &self.l0[*idx];

            let data = read_sstable(&table.path)?;

            for (k, v) in data {
                merged.insert(k, v);
            }
        }

        let sorted: Vec<_> = merged.into_iter().collect();

        let path = format!("sst_l1_{}.bin", next_sstable_id());

        let index = write_sstable(&path, &sorted)?;

        let file_size = fs::metadata(&path)?.len();

        let mut bloom = BloomFilter::with_rate(
            0.01,
            sorted.len().max(8) as u32
        );

        for (k, _) in &sorted {
            bloom.insert(k);
        }

        let min_key = sorted.first()
            .map(|(k, _)| k.clone())
            .unwrap();

        let max_key = sorted.last()
            .map(|(k, _)| k.clone())
            .unwrap();

        let table = SSTable {
            path: path.clone(),
            index,
            bloom,
            level: Level::L1,
            min_key,
            max_key,
            file_size,
        };

        // Remove candidates in descending index order so that
        // removing one doesn't shift the indices of remaining candidates.
        let mut sorted_candidates = candidates.clone();
        sorted_candidates.sort_by(|a, b| b.cmp(a));
        for idx in sorted_candidates {
            let old = self.l0.remove(idx);

            fs::remove_file(old.path)?;
        }

        self.l1.push(table);

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




